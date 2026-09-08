//! `BashRunner`: executes the unit's `.sh` script.
//!
//! Ported from `cubtera_runners::bash::BashRunner` onto the new
//! capability-aware [`RunnerStrategy`]: same script-discovery and
//! `CUBTERA_IN_<ALIAS>` env exposure, `collects_outputs = false` (a bash
//! unit must write `cubtera_outputs.json` itself, typically via
//! `[runner] outlet_command`).

use crate::error::{ExecError, ExecResult};
use crate::runner::{RunnerCapabilities, RunnerContext, RunnerStrategy};
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Runs a unit's `.sh` script with `ctx.command` as its arguments.
#[derive(Debug, Clone, Copy, Default)]
pub struct BashRunner;

impl BashRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl RunnerStrategy for BashRunner {
    fn name(&self) -> &str {
        "bash"
    }

    fn capabilities(&self) -> RunnerCapabilities {
        RunnerCapabilities {
            supports_plan_artifact: false,
            collects_outputs: false,
            pins_version: false,
            needs_identity: false,
        }
    }

    async fn binary(&self, _ctx: &RunnerContext) -> ExecResult<PathBuf> {
        Ok(PathBuf::from("bash"))
    }

    fn build_args(&self, ctx: &RunnerContext) -> ExecResult<Vec<String>> {
        // `find_script` needs to run at build-args time (build_args is
        // sync); resolve the script name synchronously via `std::fs`
        // instead - the directory listing itself is cheap and this keeps
        // the trait's `build_args` signature uniform across strategies.
        let mut entries = std::fs::read_dir(&ctx.workspace_root).map_err(ExecError::from)?;
        let script = entries
            .find_map(|e| {
                let entry = e.ok()?;
                let path = entry.path();
                (path.extension().map(|ext| ext == "sh").unwrap_or(false)).then_some(path)
            })
            .ok_or_else(|| {
                ExecError::NotFound(format!(
                    "no .sh script found in {}",
                    ctx.workspace_root.display()
                ))
            })?;

        let script_name = script
            .file_name()
            .ok_or_else(|| ExecError::NotFound("script path has no file name".to_string()))?
            .to_string_lossy()
            .into_owned();

        let mut args = vec![format!("./{script_name}")];
        args.extend(ctx.command.clone());
        Ok(args)
    }

    fn env_vars(&self, ctx: &RunnerContext) -> BTreeMap<String, String> {
        let mut env = BTreeMap::new();
        for (alias, value) in &ctx.variables {
            env.insert(
                format!("CUBTERA_IN_{}", alias.to_uppercase()),
                json_env_value(value),
            );
        }
        env
    }
}

fn json_env_value(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::ProcessRunner as _;
    use serde_json::json;

    fn ctx(workspace_root: PathBuf, command: &[&str]) -> RunnerContext {
        RunnerContext {
            workspace_root,
            command: command.iter().map(|s| s.to_string()).collect(),
            auto_approve: false,
            variables: BTreeMap::new(),
            extra_env: BTreeMap::new(),
            requested_version: None,
        }
    }

    #[test]
    fn capabilities_never_auto_collect_outputs() {
        assert!(!BashRunner::new().capabilities().collects_outputs);
    }

    #[tokio::test]
    async fn build_args_finds_sh_script_and_appends_command() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(tmp.path().join("run.sh"), "#!/bin/bash\necho hi")
            .await
            .unwrap();

        let strategy = BashRunner::new();
        let c = ctx(tmp.path().to_path_buf(), &["deploy"]);
        let args = strategy.build_args(&c).unwrap();
        assert_eq!(args[0], "./run.sh");
        assert_eq!(args[1], "deploy");
    }

    #[test]
    fn build_args_errors_when_no_script_present() {
        let tmp = tempfile::TempDir::new().unwrap();
        let strategy = BashRunner::new();
        let c = ctx(tmp.path().to_path_buf(), &[]);
        assert!(strategy.build_args(&c).is_err());
    }

    #[test]
    fn env_vars_exposes_variables_as_cubtera_in_vars() {
        let mut c = ctx(PathBuf::from("/tmp"), &[]);
        c.variables
            .insert("network".to_string(), json!({"vpc_id": "vpc-1"}));

        let env = BashRunner::new().env_vars(&c);
        assert_eq!(
            env.get("CUBTERA_IN_NETWORK"),
            Some(&r#"{"vpc_id":"vpc-1"}"#.to_string())
        );
    }

    #[tokio::test]
    async fn bash_script_actually_reads_an_injected_variable() {
        let tmp = tempfile::TempDir::new().unwrap();
        tokio::fs::write(
            tmp.path().join("run.sh"),
            "#!/bin/bash\necho \"region=$CUBTERA_IN_REGION\" > result.txt\n",
        )
        .await
        .unwrap();

        let strategy = BashRunner::new();
        let mut c = ctx(tmp.path().to_path_buf(), &["deploy"]);
        c.variables.insert("region".to_string(), json!("eu-west-1"));

        let process = crate::process::TokioProcessRunner::new();
        let binary = strategy.binary(&c).await.unwrap();
        let spec =
            crate::process::ProcessSpec::new(binary.to_string_lossy().to_string(), tmp.path())
                .with_args(strategy.build_args(&c).unwrap())
                .with_env(strategy.env_vars(&c));
        let output = process.exec(&spec).await.unwrap();
        assert!(output.success());

        let result = tokio::fs::read_to_string(tmp.path().join("result.txt"))
            .await
            .unwrap();
        assert_eq!(result.trim(), "region=eu-west-1");
    }
}
