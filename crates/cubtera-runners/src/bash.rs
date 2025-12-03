//! Bash runner

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::Runner;
use cubtera_domain::{RunParams, RunResult, Unit};
use std::process::Command;
use std::time::Instant;
use tracing::debug;

/// Bash script runner
pub struct BashRunner;

impl BashRunner {
    /// Create a new Bash runner
    pub fn new() -> Self {
        Self
    }
}

impl Default for BashRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Runner for BashRunner {
    fn name(&self) -> &str {
        "bash"
    }

    async fn init(&self) -> AppResult<()> {
        Ok(())
    }

    async fn execute(&self, unit: &Unit, params: &RunParams) -> AppResult<RunResult> {
        let start = Instant::now();

        // Find the script file
        let script_path = if let Some(source_path) = &unit.source_path {
            std::path::Path::new(source_path)
                .read_dir()
                .map_err(|e| AppError::runner(e.to_string()))?
                .filter_map(|e| e.ok())
                .find(|e| {
                    e.path()
                        .extension()
                        .map(|ext| ext == "sh")
                        .unwrap_or(false)
                })
                .map(|e| e.path())
                .ok_or_else(|| AppError::runner("No .sh script found in unit directory"))?
        } else {
            return Err(AppError::runner("Unit source path not set"));
        };

        let mut cmd = Command::new("bash");
        cmd.current_dir(&params.work_dir);
        cmd.arg(&script_path);

        // Add command arguments
        for arg in &params.command {
            cmd.arg(arg);
        }

        // Add environment variables
        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        debug!("Executing: {:?}", cmd);

        let output = cmd
            .output()
            .map_err(|e| AppError::runner(format!("Failed to execute bash: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(RunResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms,
        })
    }
}

