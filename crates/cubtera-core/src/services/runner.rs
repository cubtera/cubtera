//! Runner service
//!
//! Orchestrates runner execution with the pipeline pattern.

use crate::error::AppResult;
use crate::ports::{CopyConfig, DeploymentLogEntry, DeploymentLogRepository, RunnerFactory};
use cubtera_domain::{RunParams, RunResult, Unit};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Service for executing runners
pub struct RunnerService {
    runner_factory: Arc<dyn RunnerFactory>,
    deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
    copy_config: CopyConfig,
}

impl RunnerService {
    /// Create a new runner service
    pub fn new(runner_factory: Arc<dyn RunnerFactory>, copy_config: CopyConfig) -> Self {
        Self {
            runner_factory,
            deployment_log: None,
            copy_config,
        }
    }

    /// Set deployment log repository
    pub fn with_deployment_log(mut self, log: Arc<dyn DeploymentLogRepository>) -> Self {
        self.deployment_log = Some(log);
        self
    }

    /// Run a unit with the specified command
    ///
    /// This uses the full pipeline: copy_files → change_files → inlet → runner → outlet → logger
    pub async fn run(
        &self,
        unit: &Unit,
        command: Vec<String>,
        params_override: Option<RunParams>,
    ) -> AppResult<RunResult> {
        // Create runner based on unit's runner type
        let runner = self
            .runner_factory
            .create_runner(unit.manifest.runner_type.as_str())?;

        // Initialize runner (e.g., download terraform)
        runner.init().await?;

        // Build params
        let params = params_override.unwrap_or_else(|| {
            RunParams::new(&unit.temp_folder).with_commands(command.clone())
        });

        // Execute full pipeline
        let result = runner.run(unit, &params, &self.copy_config).await?;

        // Log deployment for apply/destroy commands
        if let Some(log) = &self.deployment_log {
            if self.should_log_command(&command) {
                let entry = self.create_log_entry(unit, &command, &result);
                if let Err(e) = log.save(&entry).await {
                    tracing::warn!("Failed to save deployment log: {}", e);
                }
            }
        }

        Ok(result)
    }

    /// Run with auto-approve (for apply/destroy)
    pub async fn run_auto_approve(
        &self,
        unit: &Unit,
        command: Vec<String>,
    ) -> AppResult<RunResult> {
        let params = RunParams::new(&unit.temp_folder)
            .with_commands(command)
            .with_auto_approve(true);

        self.run(unit, params.command.clone(), Some(params)).await
    }

    /// Check if command should be logged
    fn should_log_command(&self, command: &[String]) -> bool {
        command
            .first()
            .map(|c| matches!(c.as_str(), "apply" | "destroy"))
            .unwrap_or(false)
    }

    /// Create deployment log entry
    fn create_log_entry(
        &self,
        unit: &Unit,
        command: &[String],
        result: &RunResult,
    ) -> DeploymentLogEntry {
        DeploymentLogEntry {
            unit_name: unit.name.clone(),
            org: unit.org.clone(),
            dimensions: unit.dimensions.iter().map(|d| d.key()).collect(),
            command: command.join(" "),
            exit_code: result.exit_code.unwrap_or(-1),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            duration_ms: 0, // TODO: track duration
            git_shas: HashMap::new(),
            metadata: result.metadata.clone(),
        }
    }

    /// Get available runner types
    pub fn available_runners(&self) -> Vec<&str> {
        self.runner_factory.available_runners()
    }
}
