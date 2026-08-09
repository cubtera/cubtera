//! Terraform runner strategy
//!
//! Implements `RunnerStrategy` with terraform-specific differences:
//! - `prepare_mode`: require `init` to have run before plan/apply/destroy
//! - `extend_plan`: adds `cubtera_backend.tf` (state backend HCL)
//! - `transform_files`: converts `cubtera_*.json` to `*.auto.tfvars.json`
//! - `execute`: wraps `init` in a TCP-port lock (prevents parallel inits
//!   racing on the terraform plugin cache)

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{
    merged_env, CopyConfig, PrepareMode, ProcessRunner, ProcessSpec, RunContext, RunnerStrategy,
};
use cubtera_domain::{
    flatten_tf_outputs, MaterializationPlan, MaterializationStep, RunParams, Unit,
};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::Duration;
use tracing::{debug, info};

use super::switch as tfswitch;

/// Terraform runner strategy with version management
pub struct TerraformRunner {
    /// Default terraform version
    version: Option<String>,
    /// Lock port for init command (prevents parallel inits)
    lock_port: u16,
}

impl TerraformRunner {
    /// Create a new Terraform strategy
    pub fn new(version: Option<String>) -> Self {
        Self {
            version,
            lock_port: 65432,
        }
    }

    /// Create with custom lock port
    pub fn with_lock_port(mut self, port: u16) -> Self {
        self.lock_port = port;
        self
    }

    /// Check if command is "init"
    fn is_init_command(params: &RunParams) -> bool {
        params.command.first().map(|s| s.as_str()) == Some("init")
    }

    /// Acquire lock for init command (blocking - run via `spawn_blocking`)
    fn acquire_init_lock(lock_port: u16) -> TcpListener {
        loop {
            match TcpListener::bind(("127.0.0.1", lock_port)) {
                Ok(listener) => return listener,
                Err(_) => {
                    info!("Waiting for init lock (port {})...", lock_port);
                    let delay = rand::random::<u64>() % 400 + 800;
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }

    /// Build TF_VAR_* environment variables
    fn build_tf_vars(unit: &Unit) -> Vec<(String, String)> {
        vec![
            ("TF_VAR_org_name".to_string(), unit.org.clone()),
            ("TF_VAR_unit_name".to_string(), unit.name.clone()),
            ("TF_VAR_dim_tree".to_string(), unit.dim_tree()),
        ]
    }
}

#[async_trait]
impl RunnerStrategy for TerraformRunner {
    fn name(&self) -> &str {
        "terraform"
    }

    async fn init(&self) -> AppResult<()> {
        if let Some(version) = &self.version {
            info!("Pre-downloading Terraform {}...", version);
            let version = version.clone();
            tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
                .await
                .map_err(|e| AppError::runner(format!("Task join error: {}", e)))??;
        }
        Ok(())
    }

    fn prepare_mode(&self, params: &RunParams, copy_config: &CopyConfig) -> PrepareMode {
        if Self::is_init_command(params) {
            PrepareMode::CleanAndMaterialize
        } else {
            PrepareMode::RequireExisting {
                rematerialize: copy_config.always_copy_files,
            }
        }
    }

    fn extend_plan(&self, unit: &Unit, params: &RunParams, plan: &mut MaterializationPlan) {
        let Some(state_config) = &params.state_backend_config else {
            debug!("No state backend config, skipping backend file");
            return;
        };

        let tf_hcl = json!({ "terraform": { "backend": state_config } });
        plan.push(MaterializationStep::WriteFile {
            path: unit.temp_folder.join("cubtera_backend.tf"),
            content: json_to_hcl(&tf_hcl, 0),
        });
    }

    async fn transform_files(&self, unit: &Unit, ctx: &RunContext) -> AppResult<()> {
        let temp_folder = &unit.temp_folder;

        let mut entries = tokio::fs::read_dir(temp_folder)
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder: {}", e)))?;

        let mut json_files: Vec<PathBuf> = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder entry: {}", e)))?
        {
            let path = entry.path();
            // `cubtera_outputs.json` is a producer's *own* captured
            // `terraform output -json` (written by `collect_outputs`
            // *after* `execute`, once the run's already applied) - it must
            // never be fed back in as a declared variable/tfvars file on a
            // later command against the same temp folder (e.g. `destroy`),
            // or its keys collide with the dimension/extension variables
            // already declared from that same apply's `cubtera_dim_*.json`.
            let is_cubtera_json = path.is_file()
                && path.extension().map(|e| e == "json").unwrap_or(false)
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("cubtera_") && s != "cubtera_outputs")
                    .unwrap_or(false)
                && !path.to_string_lossy().contains(".auto.tfvars");
            if is_cubtera_json {
                json_files.push(path);
            }
        }

        if json_files.is_empty() {
            return Ok(());
        }

        let mut var_declarations = String::new();
        for file in &json_files {
            if let Ok(content) = tokio::fs::read_to_string(file).await {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(obj) = json.as_object() {
                        for key in obj.keys() {
                            var_declarations.push_str(&format!(
                                "variable \"{}\" {{\n    type        = any\n    default     = null\n    description = \"Generated by Cubtera\"\n}}\n",
                                key
                            ));
                        }
                    }
                }
            }
        }

        if !var_declarations.is_empty() {
            let vars_path = temp_folder.join("cubtera_vars.tf");
            tokio::fs::write(&vars_path, var_declarations)
                .await
                .map_err(|e| AppError::runner(format!("Failed to write vars file: {}", e)))?;
        }

        for file in &json_files {
            let new_name = format!(
                "{}.auto.tfvars.json",
                file.file_stem().unwrap().to_string_lossy()
            );
            let new_path = temp_folder.join(new_name);
            tokio::fs::rename(file, &new_path)
                .await
                .map_err(|e| AppError::runner(format!("Failed to rename file: {}", e)))?;
        }

        let _ = ctx;
        Ok(())
    }

    async fn binary(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        params: &RunParams,
    ) -> AppResult<PathBuf> {
        if let Some(custom_path) = &params.runner_command {
            info!("Using custom terraform binary: {}", custom_path);
            return Ok(PathBuf::from(custom_path));
        }

        let version = params
            .version
            .as_ref()
            .or(self.version.as_ref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "latest".to_string());

        info!("Using Terraform version: {}", version);

        tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
            .await
            .map_err(|e| AppError::runner(format!("Task join error: {}", e)))?
    }

    async fn build_args(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        params: &RunParams,
    ) -> AppResult<Vec<String>> {
        let mut args = params.command.clone();

        if params.auto_approve {
            let has_apply_or_destroy = params
                .command
                .iter()
                .any(|c| c == "apply" || c == "destroy");
            if has_apply_or_destroy {
                args.push("-auto-approve".to_string());
            }
        }

        if let Some(extra_args) = &params.extra_args {
            args.extend(extra_args.split_whitespace().map(String::from));
        }

        Ok(args)
    }

    fn env_vars(&self, unit: &Unit, _params: &RunParams) -> Vec<(String, String)> {
        let mut env = vec![
            ("TF_IN_AUTOMATION".to_string(), "true".to_string()),
            ("TF_INPUT".to_string(), "0".to_string()),
        ];
        env.extend(Self::build_tf_vars(unit));
        env
    }

    async fn execute(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
        process: &dyn ProcessRunner,
    ) -> AppResult<()> {
        let program = self.binary(unit, ctx, params).await?;
        let args = self.build_args(unit, ctx, params).await?;
        let env = merged_env(self.env_vars(unit, params), params);

        // Init needs exclusive access to the shared plugin cache; other
        // commands don't touch it and run unlocked.
        let is_init = Self::is_init_command(params);
        let _lock = if is_init {
            let lock_port = self.lock_port;
            Some(
                tokio::task::spawn_blocking(move || Self::acquire_init_lock(lock_port))
                    .await
                    .map_err(|e| AppError::runner(format!("Lock task join error: {}", e)))?,
            )
        } else {
            None
        };

        info!(
            "Executing: {} {} (in {})",
            program.display(),
            args.join(" "),
            ctx.working_dir.display()
        );

        let spec = ProcessSpec {
            program: program.clone(),
            args: args.clone(),
            working_dir: ctx.working_dir.clone(),
            env,
        };
        let output = process.exec(&spec).await?;

        ctx.exit_code = Some(output.exit_code);
        ctx.set_metadata(
            "runner",
            json!({
                "binary": program.display().to_string(),
                "command": args,
                "exit_code": output.exit_code
            }),
        );

        // Lock is released here when `_lock` goes out of scope
        Ok(())
    }

    async fn collect_outputs(
        &self,
        _unit: &Unit,
        ctx: &RunContext,
        process: &dyn ProcessRunner,
    ) -> AppResult<()> {
        // Reuse the exact binary `execute` just resolved (pinned version,
        // custom path, ...) instead of re-running version resolution.
        let binary = ctx
            .get_metadata("runner")
            .and_then(|v| v.get("binary"))
            .and_then(|v| v.as_str())
            .unwrap_or("terraform");

        let spec = ProcessSpec::shell(
            &format!("{binary} output -json > cubtera_outputs.json"),
            ctx.working_dir.clone(),
        );
        let output = process.exec(&spec).await?;
        if !output.success() {
            return Err(AppError::runner(format!(
                "'{binary} output -json' failed with exit code {}",
                output.exit_code
            )));
        }
        Ok(())
    }

    fn normalize_outputs(&self, raw: &Value) -> Value {
        flatten_tf_outputs(raw)
    }
}

/// Convert JSON to HCL format (for terraform backend config)
fn json_to_hcl(json: &Value, indent: usize) -> String {
    match json {
        Value::Object(map) => {
            let mut result = String::new();
            for (key, value) in map {
                let indentation = "  ".repeat(indent);
                match value {
                    Value::Object(inner_map) => {
                        if key == "backend" && inner_map.len() == 1 {
                            let (backend_type, backend_config) = inner_map.iter().next().unwrap();
                            result.push_str(&format!(
                                "{}{}  \"{}\" {{\n",
                                indentation, key, backend_type
                            ));
                            result.push_str(&json_to_hcl(backend_config, indent + 1));
                            result.push_str(&format!("{}}}\n", indentation));
                        } else {
                            result.push_str(&format!("{}{} {{\n", indentation, key));
                            result.push_str(&json_to_hcl(value, indent + 1));
                            result.push_str(&format!("{}}}\n", indentation));
                        }
                    }
                    Value::Array(arr) => {
                        result.push_str(&format!("{}{} = [\n", indentation, key));
                        for item in arr {
                            result.push_str(&format!(
                                "{}  {},\n",
                                indentation,
                                json_to_hcl(item, indent + 1).trim()
                            ));
                        }
                        result.push_str(&format!("{}]\n", indentation));
                    }
                    _ => {
                        result.push_str(&format!(
                            "{}{} = {}\n",
                            indentation,
                            key,
                            json_to_hcl(value, indent)
                        ));
                    }
                }
            }
            result
        }
        Value::Array(arr) => {
            format!(
                "[{}]",
                arr.iter()
                    .map(|v| json_to_hcl(v, indent))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Value::String(s) => format!("\"{}\"", s),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_domain::Manifest;

    fn unit() -> Unit {
        Unit::new("network", "cubtera", Manifest::new(vec![], "tf"))
    }

    #[test]
    fn prepare_mode_cleans_on_init() {
        let strategy = TerraformRunner::new(None);
        let params = RunParams::new(".").with_command("init");
        let copy_config = CopyConfig {
            modules_path: PathBuf::new(),
            plugins_path: PathBuf::new(),
            always_copy_files: false,
            clean_cache: false,
        };
        assert_eq!(
            strategy.prepare_mode(&params, &copy_config),
            PrepareMode::CleanAndMaterialize
        );
    }

    #[test]
    fn prepare_mode_requires_existing_on_plan() {
        let strategy = TerraformRunner::new(None);
        let params = RunParams::new(".").with_command("plan");
        let copy_config = CopyConfig {
            modules_path: PathBuf::new(),
            plugins_path: PathBuf::new(),
            always_copy_files: true,
            clean_cache: false,
        };
        assert_eq!(
            strategy.prepare_mode(&params, &copy_config),
            PrepareMode::RequireExisting {
                rematerialize: true
            }
        );
    }

    #[test]
    fn extend_plan_adds_backend_hcl_when_state_config_present() {
        let strategy = TerraformRunner::new(None);
        let unit = unit().with_temp_folder("/tmp/unit");
        let params = RunParams::new(".")
            .with_command("init")
            .with_state_backend_config(json!({"s3": {"bucket": "my-bucket"}}));

        let mut plan = MaterializationPlan::new("/tmp/unit");
        strategy.extend_plan(&unit, &params, &mut plan);

        let content = plan.steps.iter().find_map(|s| match s {
            MaterializationStep::WriteFile { path, content }
                if path == std::path::Path::new("/tmp/unit/cubtera_backend.tf") =>
            {
                Some(content)
            }
            _ => None,
        });
        assert!(content.unwrap().contains("s3"));
    }

    #[test]
    fn extend_plan_is_noop_without_state_config() {
        let strategy = TerraformRunner::new(None);
        let unit = unit().with_temp_folder("/tmp/unit");
        let params = RunParams::new(".").with_command("init");

        let mut plan = MaterializationPlan::new("/tmp/unit");
        strategy.extend_plan(&unit, &params, &mut plan);
        assert!(plan.steps.is_empty());
    }

    #[tokio::test]
    async fn transform_files_ignores_cubtera_outputs_json() {
        // `cubtera_outputs.json` is written by `collect_outputs` *after* a
        // successful apply, for publishing - it must not be picked up as
        // more dimension data on a later command against the same temp
        // folder (e.g. `destroy`), or its keys collide with variables
        // already declared from that same apply's `cubtera_dim_*.json`
        // (see the regression this guards: "Duplicate variable declaration"
        // for `dim_dc_name`/`dim_dc_meta`).
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_dim_dc.json"),
            json!({"dim_dc_name": "stg1-use2"}).to_string(),
        )
        .await
        .unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_outputs.json"),
            json!({"dim_dc_name": {"value": "stg1-use2", "type": "string"}}).to_string(),
        )
        .await
        .unwrap();

        let strategy = TerraformRunner::new(None);
        let unit = unit().with_temp_folder(tmp.path());
        let ctx = RunContext::new(tmp.path().to_path_buf());
        strategy.transform_files(&unit, &ctx).await.unwrap();

        assert!(tmp.path().join("cubtera_dim_dc.auto.tfvars.json").exists());
        // Untouched: not renamed, and not counted toward the generated
        // variable declarations.
        assert!(tmp.path().join("cubtera_outputs.json").exists());
        let vars = tokio::fs::read_to_string(tmp.path().join("cubtera_vars.tf"))
            .await
            .unwrap();
        assert_eq!(vars.matches("variable \"dim_dc_name\"").count(), 1);
    }

    #[test]
    fn build_args_appends_auto_approve_for_apply() {
        let strategy = TerraformRunner::new(None);
        let unit = unit();
        let ctx = RunContext::new(PathBuf::from("/tmp/unit"));
        let params = RunParams::new(".")
            .with_command("apply")
            .with_auto_approve(true);

        let args = tokio_test_block_on(strategy.build_args(&unit, &ctx, &params)).unwrap();
        assert_eq!(args, vec!["apply", "-auto-approve"]);
    }

    fn tokio_test_block_on<F: std::future::Future>(f: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap()
            .block_on(f)
    }
}
