//! Bash runner strategy
//!
//! Executes shell scripts from the unit directory. The only real
//! difference from the trait defaults: the binary is always `bash`, and its
//! first argument is whichever `.sh` script the unit shipped.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{RunContext, RunnerStrategy};
use cubtera_domain::{RunParams, Unit};
use std::path::PathBuf;
use tracing::info;

/// Bash script runner strategy
#[derive(Default)]
pub struct BashRunner;

impl BashRunner {
    /// Create a new Bash strategy
    pub fn new() -> Self {
        Self
    }

    async fn find_script(working_dir: &std::path::Path) -> AppResult<PathBuf> {
        let mut entries = tokio::fs::read_dir(working_dir)
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder: {}", e)))?;

        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| AppError::runner(format!("Failed to read temp folder entry: {}", e)))?
        {
            let path = entry.path();
            if path.extension().map(|ext| ext == "sh").unwrap_or(false) {
                return Ok(path);
            }
        }

        Err(AppError::runner("No .sh script found in unit directory"))
    }
}

#[async_trait]
impl RunnerStrategy for BashRunner {
    fn name(&self) -> &str {
        "bash"
    }

    async fn binary(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        _params: &RunParams,
    ) -> AppResult<PathBuf> {
        Ok(PathBuf::from("bash"))
    }

    async fn build_args(
        &self,
        _unit: &Unit,
        ctx: &RunContext,
        params: &RunParams,
    ) -> AppResult<Vec<String>> {
        let script_path = Self::find_script(&ctx.working_dir).await?;
        info!("Found script: {}", script_path.display());

        // `ProcessRunner` sets the child's cwd to `ctx.working_dir`, so the
        // script must be addressed relative to that directory - not by the
        // (possibly CWD-relative) path `find_script` used to locate it,
        // which would otherwise double up and fail to resolve.
        let script_name = script_path
            .file_name()
            .ok_or_else(|| AppError::runner("Script path has no file name"))?
            .to_string_lossy()
            .into_owned();

        let mut args = vec![format!("./{script_name}")];
        args.extend(params.command.clone());
        Ok(args)
    }

    fn env_vars(&self, unit: &Unit, _params: &RunParams) -> Vec<(String, String)> {
        let mut env = vec![
            ("CUBTERA_ORG".to_string(), unit.org.clone()),
            ("CUBTERA_UNIT".to_string(), unit.name.clone()),
            ("CUBTERA_DIM_TREE".to_string(), unit.dim_tree()),
        ];
        // Same data as `cubtera_in_<alias>.json`, exposed as an env var too
        // (JSON-encoded) so a bash script can read it without a JSON parser
        // for simple cases (e.g. `jq <<< "$CUBTERA_IN_NETWORK"`).
        for (alias, value) in &unit.resolved_inputs {
            env.push((
                format!("CUBTERA_IN_{}", alias.to_uppercase()),
                serde_json::to_string(value).unwrap_or_default(),
            ));
        }
        env
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_domain::Manifest;
    use tempfile::TempDir;

    #[tokio::test]
    async fn build_args_finds_sh_script_and_appends_command() {
        let tmp = TempDir::new().unwrap();
        tokio::fs::write(tmp.path().join("run.sh"), "#!/bin/bash\necho hi")
            .await
            .unwrap();

        let strategy = BashRunner::new();
        let unit = Unit::new("script", "cubtera", Manifest::new(vec![], "bash"));
        let ctx = RunContext::new(tmp.path().to_path_buf());
        let params = RunParams::new(tmp.path()).with_command("deploy");

        let args = strategy.build_args(&unit, &ctx, &params).await.unwrap();
        assert_eq!(args[0], "./run.sh");
        assert_eq!(args[1], "deploy");
    }

    #[tokio::test]
    async fn build_args_errors_when_no_script_present() {
        let tmp = TempDir::new().unwrap();
        let strategy = BashRunner::new();
        let unit = Unit::new("script", "cubtera", Manifest::new(vec![], "bash"));
        let ctx = RunContext::new(tmp.path().to_path_buf());
        let params = RunParams::new(tmp.path());

        assert!(strategy.build_args(&unit, &ctx, &params).await.is_err());
    }

    #[test]
    fn env_vars_exposes_resolved_inputs_as_cubtera_in_vars() {
        let mut resolved_inputs = std::collections::BTreeMap::new();
        resolved_inputs.insert(
            "network".to_string(),
            serde_json::json!({"vpc_id": "vpc-1"}),
        );
        let unit = Unit::new("script", "cubtera", Manifest::new(vec![], "bash"))
            .with_resolved_inputs(resolved_inputs);

        let strategy = BashRunner::new();
        let params = RunParams::new("/tmp");
        let env = strategy.env_vars(&unit, &params);

        let (_, value) = env
            .iter()
            .find(|(k, _)| k == "CUBTERA_IN_NETWORK")
            .expect("CUBTERA_IN_NETWORK env var");
        assert_eq!(value, r#"{"vpc_id":"vpc-1"}"#);
    }
}
