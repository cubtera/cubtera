//! Bash runner
//!
//! Executes shell scripts from the unit directory.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{RunContext, Runner};
use cubtera_domain::{RunParams, Unit};
use serde_json::json;
use std::process::Command;
use tracing::info;

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

    /// Execute the bash script
    async fn runner(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        // Find the script file in temp folder
        let script_path = std::fs::read_dir(&ctx.working_dir)
            .map_err(|e| AppError::runner(format!("Failed to read temp folder: {}", e)))?
            .filter_map(|e| e.ok())
            .find(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "sh")
                    .unwrap_or(false)
            })
            .map(|e| e.path())
            .ok_or_else(|| AppError::runner("No .sh script found in unit directory"))?;

        let mut cmd = Command::new("bash");
        cmd.current_dir(&ctx.working_dir);
        cmd.arg(&script_path);

        // Add command arguments
        for arg in &params.command {
            cmd.arg(arg);
        }

        // Add environment variables
        cmd.env("CUBTERA_ORG", &unit.org);
        cmd.env("CUBTERA_UNIT", &unit.name);
        cmd.env("CUBTERA_DIM_TREE", unit.dim_tree());

        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        info!(
            "Executing: bash {} {} (in {})",
            script_path.display(),
            params.command.join(" "),
            ctx.working_dir.display()
        );

        let status = cmd
            .status()
            .map_err(|e| AppError::runner(format!("Failed to execute bash: {}", e)))?;

        let exit_code = status.code().unwrap_or(-1);
        ctx.exit_code = Some(exit_code);
        ctx.set_metadata("runner", json!({
            "script": script_path.display().to_string(),
            "command": params.command,
            "exit_code": exit_code
        }));

        Ok(())
    }

    async fn init(&self) -> AppResult<()> {
        Ok(())
    }
}
