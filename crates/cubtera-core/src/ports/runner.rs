//! Runner port (interface)
//!
//! Trait for infrastructure runners with a pipeline pattern.
//! Each runner can override specific pipeline steps while inheriting defaults.
//!
//! Pipeline order:
//! 1. copy_files   - Copy unit files to temp folder
//! 2. change_files - Transform files (e.g., JSON → tfvars)
//! 3. inlet        - Pre-command execution
//! 4. runner       - Main command execution
//! 5. outlet       - Post-command execution
//! 6. logger       - Logging/audit

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::{RunParams, RunResult, Unit};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Context passed through pipeline steps
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    /// Working directory for runner execution
    pub working_dir: PathBuf,
    /// Exit code from runner command
    pub exit_code: Option<i32>,
    /// Metadata collected during pipeline
    pub metadata: HashMap<String, Value>,
}

impl RunContext {
    /// Create new context with working directory
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            exit_code: None,
            metadata: HashMap::new(),
        }
    }

    /// Update metadata
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }
}

/// Configuration for copy_files step
#[derive(Debug, Clone)]
pub struct CopyConfig {
    /// Path to modules directory
    pub modules_path: PathBuf,
    /// Path to plugins directory
    pub plugins_path: PathBuf,
    /// Always copy files (not just on init)
    pub always_copy_files: bool,
    /// Clean temp cache after successful run
    pub clean_cache: bool,
}

/// Runner for executing infrastructure code
///
/// Implements a pipeline pattern where each step can be overridden.
/// The `run` method executes all steps in order.
#[async_trait]
pub trait Runner: Send + Sync {
    /// Get the runner name for logging
    fn name(&self) -> &str;

    // ============ PIPELINE STEPS ============
    // Each step can be overridden by specific runners.
    // Default implementations provide reasonable behavior.

    /// Step 1: Copy files to temp folder
    ///
    /// Default: Remove temp folder and copy unit files.
    /// Override: Terraform checks if command is "init" before removing.
    async fn copy_files(
        &self,
        unit: &Unit,
        _params: &RunParams,
        ctx: &mut RunContext,
        copy_config: &CopyConfig,
    ) -> AppResult<()> {
        // Default implementation: always remove and copy
        unit.remove_temp_folder().map_err(|e| {
            crate::error::AppError::runner(format!("Failed to remove temp folder: {}", e))
        })?;

        unit.copy_files_to_temp(&copy_config.modules_path, &copy_config.plugins_path)
            .map_err(|e| {
                crate::error::AppError::runner(format!("Failed to copy files: {}", e))
            })?;

        ctx.working_dir = unit.temp_folder.clone();
        ctx.set_metadata("copy_files", serde_json::json!("executed"));
        Ok(())
    }

    /// Step 2: Transform files (e.g., JSON → tfvars)
    ///
    /// Default: No-op.
    /// Override: Terraform converts cubtera_*.json to .auto.tfvars.json
    async fn change_files(
        &self,
        _unit: &Unit,
        _params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        ctx.set_metadata("change_files", serde_json::json!("passed"));
        Ok(())
    }

    /// Step 3: Pre-command execution
    ///
    /// Default: Execute inlet_command if set in params.
    async fn inlet(
        &self,
        _unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        if let Some(cmd) = &params.inlet_command {
            let exit_code = execute_shell_command(cmd, &ctx.working_dir)?;
            ctx.set_metadata("inlet", serde_json::json!({
                "command": cmd,
                "exit_code": exit_code
            }));
            if exit_code != 0 {
                return Err(crate::error::AppError::runner(format!(
                    "Inlet command failed with exit code: {}",
                    exit_code
                )));
            }
        } else {
            ctx.set_metadata("inlet", serde_json::json!("skipped"));
        }
        Ok(())
    }

    /// Step 4: Main runner execution
    ///
    /// MUST be overridden by each runner implementation.
    async fn runner(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()>;

    /// Step 5: Post-command execution
    ///
    /// Default: Execute outlet_command if set in params.
    async fn outlet(
        &self,
        _unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        if let Some(cmd) = &params.outlet_command {
            let exit_code = execute_shell_command(cmd, &ctx.working_dir)?;
            ctx.set_metadata("outlet", serde_json::json!({
                "command": cmd,
                "exit_code": exit_code
            }));
            if exit_code != 0 {
                return Err(crate::error::AppError::runner(format!(
                    "Outlet command failed with exit code: {}",
                    exit_code
                )));
            }
        } else {
            ctx.set_metadata("outlet", serde_json::json!("skipped"));
        }
        Ok(())
    }

    /// Step 6: Logging and audit
    ///
    /// Default: Log execution result.
    /// Override: Send to deployment log database.
    async fn logger(
        &self,
        _unit: &Unit,
        _params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        ctx.set_metadata("logger", serde_json::json!("passed"));
        tracing::debug!(
            runner = self.name(),
            exit_code = ?ctx.exit_code,
            working_dir = ?ctx.working_dir,
            "Runner execution completed"
        );
        Ok(())
    }

    // ============ MAIN ENTRY POINT ============

    /// Execute the full pipeline
    ///
    /// Runs all steps in order: copy_files → change_files → inlet → runner → outlet → logger
    ///
    /// This method should generally NOT be overridden.
    async fn run(
        &self,
        unit: &Unit,
        params: &RunParams,
        copy_config: &CopyConfig,
    ) -> AppResult<RunResult> {
        let mut ctx = RunContext::new(unit.temp_folder.clone());

        tracing::info!(
            runner = self.name(),
            unit = %unit.name,
            command = ?params.command,
            "Starting runner pipeline"
        );

        self.copy_files(unit, params, &mut ctx, copy_config).await?;
        self.change_files(unit, params, &mut ctx).await?;
        self.inlet(unit, params, &mut ctx).await?;
        self.runner(unit, params, &mut ctx).await?;
        self.outlet(unit, params, &mut ctx).await?;
        self.logger(unit, params, &mut ctx).await?;

        Ok(RunResult {
            success: ctx.exit_code.unwrap_or(0) == 0,
            exit_code: ctx.exit_code,
            output: None,
            metadata: ctx.metadata,
        })
    }

    // ============ OPTIONAL LIFECYCLE ============

    /// Initialize the runner (e.g., download binaries)
    async fn init(&self) -> AppResult<()> {
        Ok(())
    }
}

/// Factory for creating runners
pub trait RunnerFactory: Send + Sync {
    /// Create a runner for the given type
    fn create_runner(&self, runner_type: &str) -> AppResult<Box<dyn Runner>>;

    /// Get available runner types
    fn available_runners(&self) -> Vec<&str>;
}

/// Execute a shell command in a directory
fn execute_shell_command(command: &str, working_dir: &PathBuf) -> AppResult<i32> {
    use std::process::Command;    let output = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(working_dir)
        .spawn()
        .map_err(|e| {
            crate::error::AppError::runner(format!("Failed to spawn command: {}", e))
        })?
        .wait()
        .map_err(|e| {
            crate::error::AppError::runner(format!("Failed to wait for command: {}", e))
        })?;    Ok(output.code().unwrap_or(1))
}