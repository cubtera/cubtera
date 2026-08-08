//! Helm runner strategy
//!
//! Ported from `test2`'s `core::runner::helm` (the "helm-раннер" wave-2
//! item): before running `helm`, merge every `cubtera_*.json` file
//! materialized into the unit's temp folder (dimension data, extensions)
//! into one JSON object and render it through `values.yaml.tpl` (if the
//! unit ships one) with handlebars, writing the result to `values.yaml`.
//! Everything else - `binary`/`build_args`/`env_vars` - matches the trait
//! defaults: `helm <command...>` with the same `CUBTERA_*` env vars every
//! other strategy sets, run from the temp folder.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{RunContext, RunnerStrategy};
use cubtera_domain::{RunParams, Unit};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::info;

const VALUES_TEMPLATE_NAME: &str = "values.yaml.tpl";
const VALUES_OUTPUT_NAME: &str = "values.yaml";

/// Helm chart runner strategy
#[derive(Default)]
pub struct HelmRunner;

impl HelmRunner {
    /// Create a new Helm strategy
    pub fn new() -> Self {
        Self
    }

    /// Every `cubtera_*.json` file directly in `temp_folder` (dimension
    /// data written by [`cubtera_domain::Unit::materialize`], plus
    /// `cubtera_ext.json` when extensions are in play), merged into one
    /// object - later files (sorted by name) win on key collisions, same
    /// "children override parents" precedence the dim-vars files already
    /// encode in their own naming.
    async fn merged_dimension_data(temp_folder: &Path) -> AppResult<Value> {
        let mut entries = tokio::fs::read_dir(temp_folder)
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder: {e}")))?;

        let mut json_files: Vec<PathBuf> = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder entry: {e}")))?
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
                .map_err(|e| AppError::runner(format!("Failed to read {file:?}: {e}")))?;
            let value: Value = serde_json::from_str(&content)
                .map_err(|e| AppError::runner(format!("Invalid JSON in {file:?}: {e}")))?;
            if let Value::Object(obj) = value {
                merged.extend(obj);
            }
        }
        Ok(Value::Object(merged))
    }

    /// Render `values.yaml.tpl` (if present) with the merged dimension data
    /// and write the result to `values.yaml`. A missing template is not an
    /// error - not every chart needs generated values.
    async fn render_values_template(temp_folder: &Path, data: &Value) -> AppResult<()> {
        let template_path = temp_folder.join(VALUES_TEMPLATE_NAME);
        if !tokio::fs::try_exists(&template_path).await.unwrap_or(false) {
            info!("No {VALUES_TEMPLATE_NAME} found, skipping values.yaml generation");
            return Ok(());
        }

        let template = tokio::fs::read_to_string(&template_path)
            .await
            .map_err(|e| AppError::runner(format!("Failed to read {template_path:?}: {e}")))?;

        let mut handlebars = handlebars::Handlebars::new();
        handlebars.set_strict_mode(true);
        let rendered = handlebars.render_template(&template, data).map_err(|e| {
            AppError::runner(format!("Failed to render {VALUES_TEMPLATE_NAME}: {e}"))
        })?;

        let output_path = temp_folder.join(VALUES_OUTPUT_NAME);
        tokio::fs::write(&output_path, rendered)
            .await
            .map_err(|e| AppError::runner(format!("Failed to write {output_path:?}: {e}")))?;
        info!(
            "Rendered {} from {}",
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

    async fn transform_files(&self, unit: &Unit, _ctx: &RunContext) -> AppResult<()> {
        let temp_folder = &unit.temp_folder;
        let data = Self::merged_dimension_data(temp_folder).await?;
        Self::render_values_template(temp_folder, &data).await
    }

    async fn binary(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        _params: &RunParams,
    ) -> AppResult<PathBuf> {
        Ok(PathBuf::from("helm"))
    }

    fn env_vars(&self, unit: &Unit, _params: &RunParams) -> Vec<(String, String)> {
        vec![
            ("CUBTERA_ORG".to_string(), unit.org.clone()),
            ("CUBTERA_UNIT".to_string(), unit.name.clone()),
            ("CUBTERA_DIM_TREE".to_string(), unit.dim_tree()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_domain::Manifest;
    use serde_json::json;

    fn unit() -> Unit {
        Unit::new("chart", "cubtera", Manifest::new(vec![], "helm"))
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
    async fn transform_files_skips_rendering_without_template() {
        let tmp = tempfile::TempDir::new().unwrap();
        let strategy = HelmRunner::new();
        let unit = unit().with_temp_folder(tmp.path());
        let ctx = RunContext::new(tmp.path().to_path_buf());

        strategy.transform_files(&unit, &ctx).await.unwrap();
        assert!(!tmp.path().join(VALUES_OUTPUT_NAME).exists());
    }

    #[tokio::test]
    async fn transform_files_renders_values_yaml_from_template() {
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
        let unit = unit().with_temp_folder(tmp.path());
        let ctx = RunContext::new(tmp.path().to_path_buf());
        strategy.transform_files(&unit, &ctx).await.unwrap();

        let rendered = tokio::fs::read_to_string(tmp.path().join(VALUES_OUTPUT_NAME))
            .await
            .unwrap();
        assert_eq!(rendered, "environment: prod\n");
    }

    #[tokio::test]
    async fn binary_is_helm() {
        let strategy = HelmRunner::new();
        let unit = unit();
        let ctx = RunContext::new(PathBuf::from("/tmp/unit"));
        let params = RunParams::new(".");
        let binary = strategy.binary(&unit, &ctx, &params).await.unwrap();
        assert_eq!(binary, PathBuf::from("helm"));
    }
}
