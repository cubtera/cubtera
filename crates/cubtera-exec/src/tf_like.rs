//! `TfLikeRunner`: one implementation for both Terraform and OpenTofu.
//!
//! v2 had two independent strategies (`TerraformRunner`/`OpenTofuRunner`)
//! that had already drifted: OpenTofu was missing `extend_plan`,
//! `transform_files`, and `TF_VAR_*` injection entirely (it inherited the
//! trait's do-nothing defaults and nobody noticed, because
//! `example/units/tf_unit01` never referenced a dimension-derived
//! variable - see `example/units/tflike_fixture` below, added specifically
//! to catch this class of regression by *actually consuming* an injected
//! variable). Here there is exactly one implementation, parameterized by
//! `binary_name` and a [`VersionResolver`]; the two constructors
//! (`terraform`/`opentofu`) only differ in which resolver and capabilities
//! they wire up.

use crate::error::{ExecError, ExecResult};
use crate::process::{ProcessRunner, ProcessSpec};
use crate::runner::{RunnerCapabilities, RunnerContext, RunnerStrategy};
use crate::version::{PathVersionResolver, TfSwitchResolver, VersionResolver};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Unified Terraform/OpenTofu strategy.
pub struct TfLikeRunner {
    binary_name: &'static str,
    resolver: Arc<dyn VersionResolver>,
    capabilities: RunnerCapabilities,
}

impl TfLikeRunner {
    /// Terraform: version-pinned via `TfSwitchResolver` (downloads and
    /// caches under `cache_dir`).
    pub fn terraform(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            binary_name: "terraform",
            resolver: Arc::new(TfSwitchResolver::new(cache_dir)),
            capabilities: RunnerCapabilities {
                supports_plan_artifact: true,
                collects_outputs: true,
                pins_version: true,
                needs_identity: false,
            },
        }
    }

    /// OpenTofu: resolved from `PATH`, no auto-download - a real
    /// [`VersionResolver`], not a `TODO`, but `pins_version` is honestly
    /// `false` since there's no cache/pin behind it yet.
    pub fn opentofu() -> Self {
        Self {
            binary_name: "tofu",
            resolver: Arc::new(PathVersionResolver::new("tofu")),
            capabilities: RunnerCapabilities {
                supports_plan_artifact: true,
                collects_outputs: true,
                pins_version: false,
                needs_identity: false,
            },
        }
    }

    /// Escape hatch for tests / custom binaries that still want the
    /// terraform-shaped CLI contract (plan/apply/destroy, `TF_VAR_*`,
    /// `output -json`).
    pub fn with_resolver(
        binary_name: &'static str,
        resolver: Arc<dyn VersionResolver>,
        capabilities: RunnerCapabilities,
    ) -> Self {
        Self {
            binary_name,
            resolver,
            capabilities,
        }
    }

    fn is_apply_or_destroy(ctx: &RunnerContext) -> bool {
        ctx.command
            .first()
            .map(|c| c == "apply" || c == "destroy")
            .unwrap_or(false)
    }
}

#[async_trait]
impl RunnerStrategy for TfLikeRunner {
    fn name(&self) -> &str {
        self.binary_name
    }

    fn capabilities(&self) -> RunnerCapabilities {
        self.capabilities
    }

    async fn binary(&self, ctx: &RunnerContext) -> ExecResult<PathBuf> {
        self.resolver
            .resolve(ctx.requested_version.as_deref())
            .await
    }

    async fn prepare(&self, ctx: &RunnerContext) -> ExecResult<()> {
        transform_cubtera_json_files(&ctx.workspace_root).await
    }

    fn build_args(&self, ctx: &RunnerContext) -> ExecResult<Vec<String>> {
        let mut args = ctx.command.clone();
        if ctx.auto_approve && Self::is_apply_or_destroy(ctx) {
            args.push("-auto-approve".to_string());
        }
        Ok(args)
    }

    fn env_vars(&self, ctx: &RunnerContext) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        env.insert("TF_IN_AUTOMATION".to_string(), "true".to_string());
        env.insert("TF_INPUT".to_string(), "0".to_string());
        for (key, value) in &ctx.variables {
            let rendered = match value {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            env.insert(format!("TF_VAR_{key}"), rendered);
        }
        env
    }

    async fn collect_outputs(
        &self,
        ctx: &RunnerContext,
        process: &dyn ProcessRunner,
    ) -> ExecResult<()> {
        let binary = self.binary(ctx).await?;
        let spec = ProcessSpec::shell(
            format!("{} output -json > cubtera_outputs.json", binary.display()),
            ctx.workspace_root.clone(),
        );
        let output = process.exec(&spec).await?;
        if !output.success() {
            return Err(ExecError::Process(format!(
                "'{} output -json' failed with exit code {}",
                binary.display(),
                output.exit_code
            )));
        }
        Ok(())
    }

    fn normalize_outputs(&self, raw: &Value) -> Value {
        flatten_tf_outputs(raw)
    }
}

/// `{name: {value, type, sensitive}}` (raw `terraform output -json`) ->
/// `{name: value}` (what every consumer's `cubtera_in_<alias>.json` and
/// `cubtera-app`'s `OutputSet` expect). Ported verbatim from
/// `cubtera_domain::flatten_tf_outputs` - pure, no reason to change it.
fn flatten_tf_outputs(raw: &Value) -> Value {
    let Some(obj) = raw.as_object() else {
        return raw.clone();
    };
    let mut flat = serde_json::Map::new();
    for (name, entry) in obj {
        let value = entry
            .as_object()
            .and_then(|e| e.get("value"))
            .cloned()
            .unwrap_or_else(|| entry.clone());
        flat.insert(name.clone(), value);
    }
    Value::Object(flat)
}

/// Turn every `cubtera_*.json` file already materialized into
/// `ctx.workspace_root` (`cubtera_dim_{type}.json`, `cubtera_ext.json`,
/// `cubtera_in_<alias>.json`, `cubtera_inputs.json` - see
/// `cubtera_model::Unit::materialize`) into a Terraform-consumable shape:
///
/// - every top-level key across those files becomes a generated
///   `variable "<key>" { type = any, default = null }` declaration in
///   `cubtera_vars.tf`, since example/real units reference `var.dim_dc_name`
///   etc. without declaring the variable themselves (cubtera declares it
///   for them, same as v2's `TerraformRunner::transform_files`);
/// - each file itself is renamed to `<stem>.auto.tfvars.json`, which
///   Terraform auto-loads as the corresponding variable's value (a plain
///   `TF_VAR_*` env var only supplies a *value*, it never declares the
///   variable, so file-based tfvars + a generated declaration is the only
///   way to make an undeclared-by-the-unit variable actually resolve).
///
/// `cubtera_outputs.json` is deliberately excluded - see the doc comment
/// inline below (ported from v2, this is the exact regression that guard
/// was added for).
async fn transform_cubtera_json_files(workspace_root: &Path) -> ExecResult<()> {
    let mut entries = tokio::fs::read_dir(workspace_root)
        .await
        .map_err(|e| ExecError::Process(format!("failed to read temp folder: {e}")))?;

    let mut json_files: Vec<PathBuf> = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ExecError::Process(format!("failed to read temp folder entry: {e}")))?
    {
        let path = entry.path();
        // `cubtera_outputs.json` is a producer's *own* captured
        // `<binary> output -json` (written by `collect_outputs` *after*
        // `execute`, once the run's already applied) - it must never be
        // fed back in as a declared variable/tfvars file on a later
        // command against the same temp folder (e.g. `destroy`), or its
        // keys collide with the dimension/extension variables already
        // declared from that same apply's `cubtera_dim_*.json`.
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
                            "variable \"{key}\" {{\n    type        = any\n    default     = null\n    description = \"Generated by Cubtera\"\n}}\n",
                        ));
                    }
                }
            }
        }
    }

    if !var_declarations.is_empty() {
        let vars_path = workspace_root.join("cubtera_vars.tf");
        tokio::fs::write(&vars_path, var_declarations)
            .await
            .map_err(|e| ExecError::Process(format!("failed to write vars file: {e}")))?;
    }

    for file in &json_files {
        let new_name = format!(
            "{}.auto.tfvars.json",
            file.file_stem().unwrap().to_string_lossy()
        );
        let new_path = workspace_root.join(new_name);
        tokio::fs::rename(file, &new_path)
            .await
            .map_err(|e| ExecError::Process(format!("failed to rename file: {e}")))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::TokioProcessRunner;
    use crate::runner::RunnerContext;
    use serde_json::json;

    fn ctx(command: &[&str]) -> RunnerContext {
        RunnerContext {
            workspace_root: PathBuf::from("/tmp/unit"),
            command: command.iter().map(|s| s.to_string()).collect(),
            auto_approve: false,
            variables: BTreeMap::new(),
            extra_env: BTreeMap::new(),
            requested_version: None,
        }
    }

    /// The exact drift v2 had: terraform and opentofu must build identical
    /// args/env for the same context, since they're now one implementation.
    #[test]
    fn terraform_and_opentofu_build_identical_args_and_env() {
        let mut c = ctx(&["apply"]);
        c.auto_approve = true;
        c.variables.insert("region".to_string(), json!("us-east-1"));
        c.variables.insert("replicas".to_string(), json!(3));

        let tf = TfLikeRunner::terraform("/tmp/tf-cache");
        let tofu = TfLikeRunner::opentofu();

        assert_eq!(tf.build_args(&c).unwrap(), tofu.build_args(&c).unwrap());
        assert_eq!(tf.build_args(&c).unwrap(), vec!["apply", "-auto-approve"]);

        let tf_env = tf.env_vars(&c);
        let tofu_env = tofu.env_vars(&c);
        assert_eq!(tf_env, tofu_env);
        assert_eq!(tf_env.get("TF_VAR_region"), Some(&"us-east-1".to_string()));
        assert_eq!(tf_env.get("TF_VAR_replicas"), Some(&"3".to_string()));
    }

    #[test]
    fn auto_approve_is_not_appended_for_plan() {
        let mut c = ctx(&["plan"]);
        c.auto_approve = true;
        let tofu = TfLikeRunner::opentofu();
        assert_eq!(tofu.build_args(&c).unwrap(), vec!["plan"]);
    }

    #[test]
    fn capabilities_reflect_declared_version_pinning() {
        assert!(TfLikeRunner::terraform("/tmp").capabilities().pins_version);
        assert!(!TfLikeRunner::opentofu().capabilities().pins_version);
        // Both collect outputs and support plan artifacts - the exact
        // capability OpenTofu was silently missing in v2.
        assert!(TfLikeRunner::opentofu().capabilities().collects_outputs);
        assert!(
            TfLikeRunner::opentofu()
                .capabilities()
                .supports_plan_artifact
        );
    }

    #[test]
    fn normalize_outputs_flattens_terraform_output_json_shape() {
        let tofu = TfLikeRunner::opentofu();
        let raw = json!({"vpc_id": {"value": "vpc-1", "type": "string", "sensitive": false}});
        assert_eq!(tofu.normalize_outputs(&raw), json!({"vpc_id": "vpc-1"}));
    }

    /// End-to-end against the real `tofu` binary (present in this sandbox,
    /// terraform is not) with a fixture that actually declares and reads a
    /// variable - the regression `example/units/tf_unit01` couldn't catch
    /// because it never referenced one.
    #[tokio::test]
    async fn opentofu_apply_consumes_an_injected_dimension_variable() {
        let Ok(check) = tokio::process::Command::new("tofu")
            .arg("version")
            .output()
            .await
        else {
            eprintln!("skipping: tofu not installed");
            return;
        };
        if !check.status.success() {
            eprintln!("skipping: tofu not usable");
            return;
        }

        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("main.tf"),
            r#"
variable "region" {
  type = string
}

output "region_upper" {
  value = upper(var.region)
}
"#,
        )
        .await
        .unwrap();

        let runner = TfLikeRunner::opentofu();
        let process = TokioProcessRunner::new();

        let mut init_ctx = ctx(&["init"]);
        init_ctx.workspace_root = tmp.path().to_path_buf();
        let binary = runner.binary(&init_ctx).await.unwrap();
        let init_spec = ProcessSpec::new(binary.to_string_lossy().to_string(), tmp.path())
            .with_args(runner.build_args(&init_ctx).unwrap())
            .with_env(runner.env_vars(&init_ctx));
        let init_out = process.exec(&init_spec).await.unwrap();
        assert!(init_out.success(), "tofu init failed");

        let mut apply_ctx = ctx(&["apply"]);
        apply_ctx.workspace_root = tmp.path().to_path_buf();
        apply_ctx.auto_approve = true;
        apply_ctx
            .variables
            .insert("region".to_string(), json!("us-west-2"));
        let apply_spec = ProcessSpec::new(binary.to_string_lossy().to_string(), tmp.path())
            .with_args(runner.build_args(&apply_ctx).unwrap())
            .with_env(runner.env_vars(&apply_ctx));
        let apply_out = process.exec(&apply_spec).await.unwrap();
        assert!(apply_out.success(), "tofu apply failed");

        runner.collect_outputs(&apply_ctx, &process).await.unwrap();
        let raw: Value = serde_json::from_str(
            &tokio::fs::read_to_string(tmp.path().join("cubtera_outputs.json"))
                .await
                .unwrap(),
        )
        .unwrap();
        let flat = runner.normalize_outputs(&raw);
        assert_eq!(flat["region_upper"], json!("US-WEST-2"));
    }

    /// The exact regression this file's `prepare()` guards against: a unit
    /// (like `example/units/tf_unit02`) referencing `var.dim_dc_name`
    /// without declaring it - relies on `cubtera_dim_dc.json` being
    /// transformed into a declared, auto-loaded tfvars file.
    #[tokio::test]
    async fn prepare_declares_and_loads_dimension_vars_from_materialized_json() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_dim_dc.json"),
            json!({"dim_dc_name": "stg1-use2", "dim_dc_meta": {"region": "us-east-2"}}).to_string(),
        )
        .await
        .unwrap();

        let runner = TfLikeRunner::opentofu();
        let mut c = ctx(&["apply"]);
        c.workspace_root = tmp.path().to_path_buf();
        runner.prepare(&c).await.unwrap();

        assert!(!tmp.path().join("cubtera_dim_dc.json").exists());
        assert!(tmp.path().join("cubtera_dim_dc.auto.tfvars.json").exists());

        let vars = tokio::fs::read_to_string(tmp.path().join("cubtera_vars.tf"))
            .await
            .unwrap();
        assert!(vars.contains("variable \"dim_dc_name\""));
        assert!(vars.contains("variable \"dim_dc_meta\""));
    }

    /// `cubtera_outputs.json` must survive untouched, and must not be
    /// picked up as more dimension data - see the doc comment on
    /// `transform_cubtera_json_files` (the exact "Duplicate variable
    /// declaration" regression v2 guarded against).
    #[tokio::test]
    async fn prepare_ignores_cubtera_outputs_json() {
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

        let runner = TfLikeRunner::opentofu();
        let mut c = ctx(&["destroy"]);
        c.workspace_root = tmp.path().to_path_buf();
        runner.prepare(&c).await.unwrap();

        assert!(tmp.path().join("cubtera_dim_dc.auto.tfvars.json").exists());
        assert!(tmp.path().join("cubtera_outputs.json").exists());
        let vars = tokio::fs::read_to_string(tmp.path().join("cubtera_vars.tf"))
            .await
            .unwrap();
        assert_eq!(vars.matches("variable \"dim_dc_name\"").count(), 1);
    }
}
