//! Bridges v3's `cubtera_app::ports::Executor` onto `cubtera-exec`'s
//! `RunnerStrategy`, capturing combined stdout+stderr instead of
//! inheriting it - the opposite of `crates/cubtera/src/exec_bridge.rs`'s
//! CLI bridge, and the reason this is a separate type rather than a
//! shared one: the server has no TTY to inherit into, and capturing lets
//! `GET /v1/{org}/runs/{run_id}/log` serve a finished run's output (see
//! `cubtera_exec::process::CapturingProcessRunner`'s doc comment for why
//! the CLI must never get this behavior instead).

use async_trait::async_trait;
use cubtera_app::ports::{ExecCapabilities, ExecOutcome, ExecRequest, Executor};
use cubtera_app::{AppError, AppResult};
use cubtera_exec::{
    BashRunner, CapturingProcessRunner, ExecError, RunnerContext, RunnerStrategy, TfLikeRunner,
    TokioCapturingProcessRunner,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

pub struct ServerExecutor {
    workspace_root: PathBuf,
    tf_cache_dir: PathBuf,
    process: TokioCapturingProcessRunner,
}

impl ServerExecutor {
    pub fn new(workspace_root: PathBuf, tf_cache_dir: PathBuf) -> Self {
        Self {
            workspace_root,
            tf_cache_dir,
            process: TokioCapturingProcessRunner::new(),
        }
    }

    /// Same runner-type dispatch as the CLI bridge - see its doc comment
    /// for why `"helm"` is a deliberate gap.
    fn strategy(&self, runner_type: &str) -> AppResult<Arc<dyn RunnerStrategy>> {
        match runner_type {
            "tf" | "terraform" => Ok(Arc::new(TfLikeRunner::terraform(self.tf_cache_dir.clone()))),
            "tofu" | "opentofu" => Ok(Arc::new(TfLikeRunner::opentofu())),
            "bash" | "sh" => Ok(Arc::new(BashRunner::new())),
            other => Err(AppError::validation(format!(
                "runner type {other:?} has no cubtera-exec RunnerStrategy yet (P4 covers tf/tofu/bash only)"
            ))),
        }
    }

    fn context(&self, req: &ExecRequest) -> RunnerContext {
        RunnerContext {
            workspace_root: self.workspace_root.clone(),
            command: req.command.clone(),
            auto_approve: req.auto_approve,
            variables: req.variables.clone(),
            extra_env: Default::default(),
            requested_version: req.requested_version.clone(),
        }
    }
}

#[async_trait]
impl Executor for ServerExecutor {
    async fn capabilities(&self, runner_type: &str) -> AppResult<ExecCapabilities> {
        let caps = self.strategy(runner_type)?.capabilities();
        Ok(ExecCapabilities {
            supports_plan_artifact: caps.supports_plan_artifact,
            collects_outputs: caps.collects_outputs,
            pins_version: caps.pins_version,
            needs_identity: caps.needs_identity,
        })
    }

    async fn resolve_runner_version(
        &self,
        runner_type: &str,
        requested_version: Option<&str>,
    ) -> AppResult<String> {
        let strategy = self.strategy(runner_type)?;
        let ctx = RunnerContext {
            workspace_root: self.workspace_root.clone(),
            requested_version: requested_version.map(str::to_string),
            ..Default::default()
        };
        let binary = strategy.binary(&ctx).await.map_err(exec_error)?;
        Ok(binary.display().to_string())
    }

    async fn execute(&self, req: ExecRequest) -> AppResult<ExecOutcome> {
        let strategy = self.strategy(&req.runner_type)?;
        let ctx = self.context(&req);

        let binary = strategy.binary(&ctx).await.map_err(exec_error)?;
        let args = strategy.build_args(&ctx).map_err(exec_error)?;
        let env = cubtera_exec::merged_env(strategy.env_vars(&ctx), &ctx);

        let spec = cubtera_exec::ProcessSpec::new(
            binary.to_string_lossy().to_string(),
            ctx.workspace_root.clone(),
        )
        .with_args(args)
        .with_env(env);
        let (output, log_bytes) = self
            .process
            .exec_captured(&spec)
            .await
            .map_err(exec_error)?;

        let mut outputs: Option<Value> = None;
        if output.success() && req.collect_outputs && strategy.capabilities().collects_outputs {
            // `collect_outputs` shells out on its own (a fresh `<binary>
            // output -json > cubtera_outputs.json` process, via
            // `strategy.collect_outputs`'s own `ProcessRunner` - not
            // `self.process`), so it isn't captured into `log_bytes`
            // either; same as the CLI bridge, this is metadata collection,
            // not part of the run's own output stream.
            let inherited = cubtera_exec::TokioProcessRunner::new();
            strategy
                .collect_outputs(&ctx, &inherited)
                .await
                .map_err(exec_error)?;
            let raw_path = ctx.workspace_root.join("cubtera_outputs.json");
            let raw_bytes = tokio::fs::read(&raw_path).await.map_err(|e| {
                AppError::backend(format!(
                    "failed to read collected outputs at {}: {e}",
                    raw_path.display()
                ))
            })?;
            let raw: Value = serde_json::from_slice(&raw_bytes).map_err(|e| {
                AppError::backend(format!("collected outputs are not valid JSON: {e}"))
            })?;
            outputs = Some(strategy.normalize_outputs(&raw));
        }

        Ok(ExecOutcome {
            exit_code: output.exit_code,
            success: output.success(),
            runner_version: binary.display().to_string(),
            outputs,
            log_bytes: Some(log_bytes),
        })
    }
}

fn exec_error(e: ExecError) -> AppError {
    AppError::backend(e.to_string())
}
