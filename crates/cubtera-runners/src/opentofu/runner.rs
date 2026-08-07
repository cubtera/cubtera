//! OpenTofu runner
//!
//! Similar to Terraform runner but uses the `tofu` binary.
//! TODO: Add version management (tofuswitch) similar to terraform

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{CopyConfig, RunContext, Runner};
use cubtera_domain::{RunParams, Unit};
use serde_json::json;
use std::process::Command;
use tracing::info;

/// OpenTofu runner
pub struct OpenTofuRunner {
    version: Option<String>,
}

impl OpenTofuRunner {
    /// Create a new OpenTofu runner
    pub fn new(version: Option<String>) -> Self {
        Self { version }
    }

    /// Get the tofu binary path
    fn binary_path(&self) -> &str {
        "tofu"
    }
}

#[async_trait]
impl Runner for OpenTofuRunner {
    fn name(&self) -> &str {
        "opentofu"
    }

    /// OpenTofu uses similar copy logic to Terraform
    async fn copy_files(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
        copy_config: &CopyConfig,
    ) -> AppResult<()> {
        let is_init = params.command.first().map(|s| s.as_str()) == Some("init");

        if is_init {
            info!("Preparing temp folder for init: {}", unit.temp_folder.display());
            unit.remove_temp_folder()
                .map_err(|e| AppError::runner(format!("Failed to remove temp folder: {}", e)))?;
            unit.copy_files_to_temp(&copy_config.modules_path, &copy_config.plugins_path)
                .map_err(|e| AppError::runner(format!("Failed to copy files: {}", e)))?;
        } else {
            if !unit.temp_folder_exists() {
                return Err(AppError::runner(format!(
                    "Temp folder not found: {:?}. Run 'init' first.",
                    unit.temp_folder
                )));
            }
            if copy_config.always_copy_files {
                unit.copy_files_to_temp(&copy_config.modules_path, &copy_config.plugins_path)
                    .map_err(|e| AppError::runner(format!("Failed to copy files: {}", e)))?;
            }
        }

        ctx.working_dir = unit.temp_folder.clone();
        ctx.set_metadata("copy_files", json!(if is_init { "init_copy" } else { "verified" }));
        Ok(())
    }

    /// Execute the tofu command
    async fn runner(
        &self,
        _unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        let mut cmd = Command::new(self.binary_path());
        cmd.current_dir(&ctx.working_dir);

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

        // Add extra args if specified
        if let Some(extra_args) = &params.extra_args {
            for arg in extra_args.split_whitespace() {
                cmd.arg(arg);
            }
        }

        // Add TF-specific environment variables
        cmd.env("TF_IN_AUTOMATION", "true");
        cmd.env("TF_INPUT", "0");

        // Add environment variables
        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        info!(
            "Executing: {} {} (in {})",
            self.binary_path(),
            params.command.join(" "),
            ctx.working_dir.display()
        );

        let status = cmd
            .status()
            .map_err(|e| AppError::runner(format!("Failed to execute tofu: {}", e)))?;

        let exit_code = status.code().unwrap_or(-1);
        ctx.exit_code = Some(exit_code);
        ctx.set_metadata("runner", json!({
            "binary": self.binary_path(),
            "command": params.command,
            "exit_code": exit_code
        }));

        Ok(())
    }

    async fn init(&self) -> AppResult<()> {
        if let Some(version) = &self.version {
            info!("Requested OpenTofu version: {} (TODO: implement tofuswitch)", version);
        }
        Ok(())
    }
}
