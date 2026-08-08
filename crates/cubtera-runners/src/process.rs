//! Tokio-backed `ProcessRunner` adapter
//!
//! Spawns via `tokio::process::Command` (not `std::process::Command`) so
//! waiting for the child doesn't block the async runtime - see the
//! migration plan's async-discipline rule. Stdio is left at Tokio's default,
//! which inherits the parent's file descriptors: required for interactive
//! prompts and terraform's colored output to reach the user's terminal.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{ProcessOutput, ProcessRunner, ProcessSpec};

/// Executes [`ProcessSpec`]s via `tokio::process`
#[derive(Debug, Clone, Default)]
pub struct TokioProcessRunner;

impl TokioProcessRunner {
    /// Create a new process runner
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProcessRunner for TokioProcessRunner {
    async fn exec(&self, spec: &ProcessSpec) -> AppResult<ProcessOutput> {
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.current_dir(&spec.working_dir);
        cmd.envs(&spec.env);

        let status = cmd.status().await.map_err(|e| {
            AppError::runner(format!("failed to execute {}: {e}", spec.program.display()))
        })?;

        Ok(ProcessOutput {
            exit_code: status.code().unwrap_or(-1),
        })
    }
}
