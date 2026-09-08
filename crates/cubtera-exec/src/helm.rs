//! `HelmRunner`: v3-native port of v2's `cubtera_runners::helm::HelmRunner`.
//!
//! Before running `helm`, merges every `cubtera_*.json` file materialized
//! into the unit's temp folder (dimension data, extensions, resolved
//! `[inputs.<alias>]`, all written by `cubtera_model::Unit::materialize`)
//! into one JSON object and renders it through `values.yaml.tpl` (if the
//! unit ships one) with handlebars, writing the result to `values.yaml`.
//! Everything else - `binary`/`env_vars` - matches the trait defaults:
//! `helm <command...>` with `CUBTERA_DIM_TREE` derived from `ctx.variables`
//! the same way every other strategy is handed dimension data.
//!
//! No plan/apply split (`capabilities().supports_plan_artifact` is
//! `false`, same as `BashRunner`) and no automatic output collection
//! (`collects_outputs` is `false` - a helm producer must write
//! `cubtera_outputs.json` itself, typically via `[runner] outlet_command`,
//! same as v2 - see `AGENTS.md`'s "Cross-unit state" section).

use crate::error::{ExecError, ExecResult};
use crate::runner::{RunnerCapabilities, RunnerContext, RunnerStrategy};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use tracing::info;

const VALUES_TEMPLATE_NAME: &str = "values.yaml.tpl";
const VALUES_OUTPUT_NAME: &str = "values.yaml";

/// Helm chart runner strategy.
#[derive(Default)]
pub struct HelmRunner;

impl HelmRunner {
    pub fn new() -> Self {
        Self
    }

    /// Every `cubtera_*.json` file directly in `workspace_root` (dimension
    /// data/extensions/inputs written by `Unit::materialize`), merged into
    /// one object - later files (sorted by name) win on key collisions,
    /// the same "children override parents" precedence the dim-vars files
    /// already encode in their own naming.
    async fn merged_dimension_data(workspace_root: &Path) -> ExecResult<Value> {
        let mut entries = tokio::fs::read_dir(workspace_root)
            .await
            .map_err(|e| ExecError::Io(format!("failed to read temp folder: {e}")))?;

        let mut json_files: Vec<PathBuf> = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| ExecError::Io(format!("failed to read temp folder entry: {e}")))?
        {
            let path = entry.path();
            let is_cubtera_json = path.is_file()
                && path.extension().map(|e| e == "json").unwrap_or(false)
                && path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.starts_with("cubtera_"))
                    .unwrap_or(false);
            if is_cubtera_json {
                json_files.push(path);
            }
        }
        json_files.sort();

        let mut merged = serde_json::Map::new();
        for file in &json_files {
            let content = tokio::fs::read_to_string(file)
                .await
                .map_err(|e| ExecError::Io(format!("failed to read {file:?}: {e}")))?;
            let value: Value = serde_json::from_str(&content)
                .map_err(|e| ExecError::Io(format!("invalid JSON in {file:?}: {e}")))?;
            if let Value::Object(obj) = value {
                merged.extend(obj);
            }
        }
        Ok(Value::Object(merged))
    }

    /// Render `values.yaml.tpl` (if present) with the merged dimension data
    /// and write the result to `values.yaml`. A missing template is not an
    /// error - not every chart needs generated values.
    async fn render_values_template(workspace_root: &Path, data: &Value) -> ExecResult<()> {
        let template_path = workspace_root.join(VALUES_TEMPLATE_NAME);
        if !tokio::fs::try_exists(&template_path).await.unwrap_or(false) {
            info!("no {VALUES_TEMPLATE_NAME} found, skipping values.yaml generation");
            return Ok(());
        }

        let template = tokio::fs::read_to_string(&template_path)
            .await
            .map_err(|e| ExecError::Io(format!("failed to read {template_path:?}: {e}")))?;

        let mut handlebars = handlebars::Handlebars::new();
        handlebars.set_strict_mode(true);
        let rendered = handlebars.render_template(&template, data).map_err(|e| {
            ExecError::Process(format!("failed to render {VALUES_TEMPLATE_NAME}: {e}"))
        })?;

        let output_path = workspace_root.join(VALUES_OUTPUT_NAME);
        tokio::fs::write(&output_path, rendered)
            .await
            .map_err(|e| ExecError::Io(format!("failed to write {output_path:?}: {e}")))?;
        info!(
            "rendered {} from {}",
            output_path.display(),
            VALUES_TEMPLATE_NAME
        );
        Ok(())
    }
}

#[async_trait]
impl RunnerStrategy for HelmRunner {
    fn name(&self) -> &str {
        "helm"
    }

    fn capabilities(&self) -> RunnerCapabilities {
        RunnerCapabilities::default()
    }

    async fn prepare(&self, ctx: &RunnerContext) -> ExecResult<()> {
        let data = Self::merged_dimension_data(&ctx.workspace_root).await?;
        Self::render_values_template(&ctx.workspace_root, &data).await
    }

    async fn binary(&self, _ctx: &RunnerContext) -> ExecResult<PathBuf> {
        Ok(PathBuf::from("helm"))
    }

    fn build_args(&self, ctx: &RunnerContext) -> ExecResult<Vec<String>> {
        Ok(ctx.command.clone())
    }

    fn env_vars(&self, _ctx: &RunnerContext) -> BTreeMap<String, String> {
        BTreeMap::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ctx(workspace_root: &Path) -> RunnerContext {
        RunnerContext {
            workspace_root: workspace_root.to_path_buf(),
            command: vec!["upgrade".to_string(), "--install".to_string()],
            auto_approve: false,
            variables: BTreeMap::new(),
            extra_env: BTreeMap::new(),
            requested_version: None,
        }
    }

    #[tokio::test]
    async fn merged_dimension_data_combines_all_cubtera_json_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_dim_env.json"),
            json!({"dim_env_name": "prod"}).to_string(),
        )
        .await
        .unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_ext.json"),
            json!({"ext_index": "0"}).to_string(),
        )
        .await
        .unwrap();
        tokio::fs::write(tmp.path().join("other.json"), "{}")
            .await
            .unwrap();

        let merged = HelmRunner::merged_dimension_data(tmp.path()).await.unwrap();
        assert_eq!(merged["dim_env_name"], "prod");
        assert_eq!(merged["ext_index"], "0");
    }

    #[tokio::test]
    async fn merged_dimension_data_picks_up_resolved_input_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_in_network.json"),
            json!({"in_network": {"vpc_id": "vpc-1"}}).to_string(),
        )
        .await
        .unwrap();

        let merged = HelmRunner::merged_dimension_data(tmp.path()).await.unwrap();
        assert_eq!(merged["in_network"]["vpc_id"], "vpc-1");
    }

    #[tokio::test]
    async fn prepare_skips_rendering_without_template() {
        let tmp = tempfile::TempDir::new().unwrap();
        let strategy = HelmRunner::new();
        strategy.prepare(&ctx(tmp.path())).await.unwrap();
        assert!(!tmp.path().join(VALUES_OUTPUT_NAME).exists());
    }

    #[tokio::test]
    async fn prepare_renders_values_yaml_from_template() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("cubtera_dim_env.json"),
            json!({"dim_env_name": "prod"}).to_string(),
        )
        .await
        .unwrap();
        tokio::fs::write(
            tmp.path().join(VALUES_TEMPLATE_NAME),
            "environment: {{dim_env_name}}\n",
        )
        .await
        .unwrap();

        let strategy = HelmRunner::new();
        strategy.prepare(&ctx(tmp.path())).await.unwrap();

        let rendered = tokio::fs::read_to_string(tmp.path().join(VALUES_OUTPUT_NAME))
            .await
            .unwrap();
        assert_eq!(rendered, "environment: prod\n");
    }

    #[tokio::test]
    async fn binary_is_helm() {
        let strategy = HelmRunner::new();
        let tmp = tempfile::TempDir::new().unwrap();
        let binary = strategy.binary(&ctx(tmp.path())).await.unwrap();
        assert_eq!(binary, PathBuf::from("helm"));
    }

    #[test]
    fn capabilities_have_no_plan_or_output_collection() {
        let caps = HelmRunner::new().capabilities();
        assert!(!caps.supports_plan_artifact);
        assert!(!caps.collects_outputs);
        assert!(!caps.pins_version);
    }

    #[test]
    fn build_args_passes_command_through_unchanged() {
        let strategy = HelmRunner::new();
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(
            strategy.build_args(&ctx(tmp.path())).unwrap(),
            vec!["upgrade".to_string(), "--install".to_string()]
        );
    }
}
