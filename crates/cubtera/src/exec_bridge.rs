//! Bridges v3's `cubtera_app::ports::Executor` onto `cubtera-exec`'s
//! `RunnerStrategy`/`ProcessRunner`/`Workspace`.
//!
//! Same rationale and shape as `app_bridge::InventoryPortBridge` (P3):
//! `cubtera-app` cannot depend on `cubtera-exec` directly (see the crate
//! table in docs/specs/2026-09-03-cubtera-v3-architecture.md §3), so this
//! is the one place that actually owns a concrete `RunnerStrategy` and
//! translates `RunUseCase`'s `ExecRequest`/`ExecOutcome` onto it.
//!
//! One `ExecutorBridge` is scoped to a single unit's materialized
//! workspace (`workspace_root`) - `RunUseCase::plan`/`apply` always run
//! against exactly one instance's temp folder per invocation, so there is
//! no need for this bridge to be long-lived or shared across units.

use async_trait::async_trait;
use cubtera_app::ports::{ExecCapabilities, ExecOutcome, ExecRequest, Executor};
use cubtera_app::{AppError, AppResult};
use cubtera_exec::{
    BashRunner, ExecError, HelmRunner, ProcessRunner, RunnerContext, RunnerStrategy, TfLikeRunner,
    TokioProcessRunner,
};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::Arc;

/// Owns the real execution adapters for one run: which `RunnerStrategy` to
/// dispatch to (by `runner_type`), where its workspace lives, and where a
/// version-pinning strategy (`TfLikeRunner::terraform`) should cache
/// downloaded binaries.
pub struct ExecutorBridge {
    workspace_root: PathBuf,
    tf_cache_dir: PathBuf,
    process: TokioProcessRunner,
}

impl ExecutorBridge {
    pub fn new(workspace_root: PathBuf, tf_cache_dir: PathBuf) -> Self {
        Self {
            workspace_root,
            tf_cache_dir,
            process: TokioProcessRunner::new(),
        }
    }

    /// Resolve `runner_type` (the same strings `Manifest::runner_type().as_str()`
    /// already produces: `"tf"`/`"tofu"`/`"bash"`/`"helm"`) to a concrete
    /// [`RunnerStrategy`].
    fn strategy(&self, runner_type: &str) -> AppResult<Arc<dyn RunnerStrategy>> {
        match runner_type {
                "tf" | "terraform" => Ok(Arc::new(TfLikeRunner::terraform(
                self.tf_cache_dir.clone(),
            ))),
            "tofu" | "opentofu" => Ok(Arc::new(TfLikeRunner::opentofu())),
            "bash" | "sh" => Ok(Arc::new(BashRunner::new())),
            "helm" => Ok(Arc::new(HelmRunner::new())),
            other => Err(AppError::validation(format!(
                "runner type {other:?} has no cubtera-exec RunnerStrategy (known types: tf, tofu, bash, helm)"
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
impl Executor for ExecutorBridge {
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

        strategy.prepare(&ctx).await.map_err(exec_error)?;
        let binary = strategy.binary(&ctx).await.map_err(exec_error)?;
        let args = strategy.build_args(&ctx).map_err(exec_error)?;
        let env = cubtera_exec::merged_env(strategy.env_vars(&ctx), &ctx);

        let spec = cubtera_exec::ProcessSpec::new(
            binary.to_string_lossy().to_string(),
            ctx.workspace_root.clone(),
        )
        .with_args(args)
        .with_env(env);
        let output = self.process.exec(&spec).await.map_err(exec_error)?;

        let mut outputs: Option<Value> = None;
        if output.success() && req.collect_outputs && strategy.capabilities().collects_outputs {
            strategy
                .collect_outputs(&ctx, &self.process)
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
            // The CLI always inherits stdio for interactive
            // prompts/colored output - see `cubtera_exec::process`'s doc
            // comment - so there is nothing to capture here.
            log_bytes: None,
        })
    }
}

fn exec_error(e: ExecError) -> AppError {
    AppError::backend(e.to_string())
}
