//! Terraform runner
//!
//! Implements the Runner trait with terraform-specific pipeline overrides:
//! - copy_files: Different behavior for init vs plan/apply
//! - change_files: Convert cubtera_*.json to .auto.tfvars.json
//! - runner: Execute terraform with version management

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{CopyConfig, RunContext, Runner};
use cubtera_domain::{RunParams, Unit};
use serde_json::{json, Value};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use tracing::{debug, info};

use super::switch as tfswitch;

/// Terraform runner with version management
pub struct TerraformRunner {
    /// Default terraform version
    version: Option<String>,
    /// Lock port for init command (prevents parallel inits)
    lock_port: u16,
}

impl TerraformRunner {
    /// Create a new Terraform runner
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

    /// Get the terraform binary path, downloading if necessary
    async fn get_binary_path(&self, params: &RunParams) -> AppResult<PathBuf> {
        // Check if custom runner_command is specified in params
        if let Some(custom_path) = &params.runner_command {
            info!("Using custom terraform binary: {}", custom_path);
            return Ok(PathBuf::from(custom_path));
        }

        // Use version from params, or from runner config, or "latest"
        let version = params
            .version
            .as_ref()
            .or(self.version.as_ref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "latest".to_string());

        info!("Using Terraform version: {}", version);

        // Run blocking tfswitch in a separate thread to avoid runtime conflicts
        tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
            .await
            .map_err(|e| AppError::runner(format!("Task join error: {}", e)))?
    }

    /// Acquire lock for init command
    fn acquire_init_lock(&self) -> Option<TcpListener> {
        let delay = rand::random::<u64>() % 400 + 800;

        loop {
            match TcpListener::bind(("127.0.0.1", self.lock_port)) {
                Ok(listener) => return Some(listener),
                Err(_) => {
                    info!("Waiting for init lock (port {})...", self.lock_port);
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }

    /// Check if command is "init"
    fn is_init_command(params: &RunParams) -> bool {
        params.command.first().map(|s| s.as_str()) == Some("init")
    }

    /// Create state backend HCL file
    fn create_state_backend(&self, unit: &Unit, params: &RunParams) -> AppResult<()> {
        let state_config = match &params.state_backend_config {
            Some(config) => config.clone(),
            None => {
                debug!("No state backend config, skipping backend file creation");
                return Ok(());
            }
        };

        let tf_hcl = json!({
            "terraform": {
                "backend": state_config
            }
        });

        let path = unit.temp_folder.join("cubtera_backend.tf");
        let hcl_content = json_to_hcl(&tf_hcl, 0);
        std::fs::write(&path, hcl_content)
            .map_err(|e| AppError::runner(format!("Failed to write state backend: {}", e)))?;

        debug!("Created state backend file: {}", path.display());
        Ok(())
    }

    /// Build TF_VAR_* environment variables
    fn build_tf_vars(&self, unit: &Unit) -> Vec<(String, String)> {
        let mut vars = Vec::new();

        // Standard cubtera variables
        vars.push(("TF_VAR_org_name".to_string(), unit.org.clone()));
        vars.push(("TF_VAR_unit_name".to_string(), unit.name.clone()));
        vars.push(("TF_VAR_dim_tree".to_string(), unit.dim_tree()));

        vars
    }
}

#[async_trait]
impl Runner for TerraformRunner {
    fn name(&self) -> &str {
        "terraform"
    }

    /// Step 1: Copy files - different behavior for init vs other commands
    async fn copy_files(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
        copy_config: &CopyConfig,
    ) -> AppResult<()> {
        let is_init = Self::is_init_command(params);

        if is_init {
            // init: Remove temp folder and copy fresh
            info!("Preparing temp folder for init: {}", unit.temp_folder.display());

            unit.remove_temp_folder()
                .map_err(|e| AppError::runner(format!("Failed to remove temp folder: {}", e)))?;

            unit.copy_files_to_temp(&copy_config.modules_path, &copy_config.plugins_path)
                .map_err(|e| AppError::runner(format!("Failed to copy files: {}", e)))?;

            // Create state backend file
            self.create_state_backend(unit, params)?;
        } else {
            // plan/apply/destroy: Check temp folder exists
            if !unit.temp_folder_exists() {
                return Err(AppError::runner(format!(
                    "Temp folder not found: {:?}. Run 'init' first.",
                    unit.temp_folder
                )));
            }

            // Optionally re-copy files
            if copy_config.always_copy_files {
                info!("Re-copying files (always_copy_files=true)");
                unit.copy_files_to_temp(&copy_config.modules_path, &copy_config.plugins_path)
                    .map_err(|e| AppError::runner(format!("Failed to copy files: {}", e)))?;
                self.create_state_backend(unit, params)?;
            }
        }

        ctx.working_dir = unit.temp_folder.clone();
        ctx.set_metadata("copy_files", json!(if is_init { "init_copy" } else { "verified" }));
        Ok(())
    }

    /// Step 2: Transform cubtera_*.json to .auto.tfvars.json
    async fn change_files(
        &self,
        unit: &Unit,
        _params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        let temp_folder = &unit.temp_folder;

        // Find all cubtera_*.json files
        let json_files: Vec<PathBuf> = std::fs::read_dir(temp_folder)
            .map_err(|e| AppError::runner(format!("Failed to read temp folder: {}", e)))?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.is_file()
                    && p.extension().map(|e| e == "json").unwrap_or(false)
                    && p.file_stem()
                        .and_then(|s| s.to_str())
                        .map(|s| s.starts_with("cubtera_"))
                        .unwrap_or(false)
                    && !p.to_string_lossy().contains(".auto.tfvars")
            })
            .collect();

        if json_files.is_empty() {
            ctx.set_metadata("change_files", json!("no_files"));
            return Ok(());
        }

        // Generate variable declarations
        let mut var_declarations = String::new();
        for file in &json_files {
            if let Ok(content) = std::fs::read_to_string(file) {
                if let Ok(json) = serde_json::from_str::<Value>(&content) {
                    if let Some(obj) = json.as_object() {
                        for key in obj.keys() {
                            var_declarations.push_str(&format!(
                                r#"variable "{}" {{
    type        = any
    default     = null
    description = "Generated by Cubtera"
}}
"#,
                                key
                            ));
                        }
                    }
                }
            }
        }

        // Write cubtera_vars.tf
        if !var_declarations.is_empty() {
            let vars_path = temp_folder.join("cubtera_vars.tf");
            std::fs::write(&vars_path, var_declarations)
                .map_err(|e| AppError::runner(format!("Failed to write vars file: {}", e)))?;
        }

        // Rename .json to .auto.tfvars.json
        for file in &json_files {
            let new_name = format!(
                "{}.auto.tfvars.json",
                file.file_stem().unwrap().to_string_lossy()
            );
            let new_path = temp_folder.join(new_name);
            std::fs::rename(file, &new_path)
                .map_err(|e| AppError::runner(format!("Failed to rename file: {}", e)))?;
        }

        ctx.set_metadata("change_files", json!({"converted": json_files.len()}));
        Ok(())
    }

    /// Step 4: Execute terraform command
    async fn runner(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        // Get terraform binary
        let tf_path = self.get_binary_path(params).await?;

        // Check if this is an init command - needs locking
        let is_init = Self::is_init_command(params);
        let _lock = if is_init {
            self.acquire_init_lock()
        } else {
            None
        };

        let mut cmd = Command::new(&tf_path);
        cmd.current_dir(&ctx.working_dir);

        // Add command arguments
        for arg in &params.command {
            cmd.arg(arg);
        }

        // Add auto-approve for apply/destroy
        if params.auto_approve {
            let has_apply_or_destroy = params
                .command
                .iter()
                .any(|c| c == "apply" || c == "destroy");
            if has_apply_or_destroy {
                cmd.arg("-auto-approve");
            }
        }

        // Add extra args if specified
        if let Some(extra_args) = &params.extra_args {
            for arg in extra_args.split_whitespace() {
                cmd.arg(arg);
            }
        }

        // Add TF-specific environment variables
        cmd.env("TF_IN_AUTOMATION", "true");
        cmd.env("TF_INPUT", "0");

        // Add cubtera TF_VAR_* variables
        for (key, value) in self.build_tf_vars(unit) {
            cmd.env(key, value);
        }

        // Add user-specified environment variables
        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        info!(
            "Executing: {} {} (in {})",
            tf_path.display(),
            params.command.join(" "),
            ctx.working_dir.display()
        );

        // Execute with inherited stdio for interactive output
        let status = cmd
            .status()
            .map_err(|e| AppError::runner(format!("Failed to execute terraform: {}", e)))?;

        let exit_code = status.code().unwrap_or(-1);
        ctx.exit_code = Some(exit_code);
        ctx.set_metadata("runner", json!({
            "binary": tf_path.display().to_string(),
            "command": params.command,
            "exit_code": exit_code
        }));

        // Lock is automatically released here when _lock goes out of scope
        Ok(())
    }

    async fn init(&self) -> AppResult<()> {
        // Pre-download terraform if version is known
        if let Some(version) = &self.version {
            info!("Pre-downloading Terraform {}...", version);
            let version = version.clone();
            tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
                .await
                .map_err(|e| AppError::runner(format!("Task join error: {}", e)))??;
        }
        Ok(())
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
                            // Special handling for backend type
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
