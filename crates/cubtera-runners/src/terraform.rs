//! Terraform runner

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::Runner;
use cubtera_domain::{RunParams, RunResult, Unit};
use std::process::Command;
use std::time::Instant;
use tracing::{debug, info};

/// Terraform runner
pub struct TerraformRunner {
    version: Option<String>,
}

impl TerraformRunner {
    /// Create a new Terraform runner
    pub fn new(version: Option<String>) -> Self {
        Self { version }
    }

    /// Get the terraform binary path
    fn binary_path(&self) -> &str {
        "terraform"
    }
}

#[async_trait]
impl Runner for TerraformRunner {
    fn name(&self) -> &str {
        "terraform"
    }

    async fn init(&self) -> AppResult<()> {
        // TODO: Implement tfswitch-style version management
        if let Some(version) = &self.version {
            info!("Requested Terraform version: {}", version);
        }
        Ok(())
    }

    async fn execute(&self, unit: &Unit, params: &RunParams) -> AppResult<RunResult> {
        let start = Instant::now();

        let mut cmd = Command::new(self.binary_path());
        cmd.current_dir(&params.work_dir);

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

        // Add environment variables
        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        debug!("Executing: {:?}", cmd);

        let output = cmd
            .output()
            .map_err(|e| AppError::runner(format!("Failed to execute terraform: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(RunResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms,
        })
    }
}

