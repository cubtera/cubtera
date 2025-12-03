//! Runner service

use crate::error::AppResult;
use crate::ports::{DeploymentLogEntry, DeploymentLogRepository, Runner, RunnerFactory};
use cubtera_domain::{RunParams, RunResult, Unit};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Service for executing runners
pub struct RunnerService {
    runner_factory: Arc<dyn RunnerFactory>,
    deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
}

impl RunnerService {
    /// Create a new runner service
    pub fn new(runner_factory: Arc<dyn RunnerFactory>) -> Self {
        Self {
            runner_factory,
            deployment_log: None,
        }
    }

    /// Set deployment log repository
    pub fn with_deployment_log(mut self, log: Arc<dyn DeploymentLogRepository>) -> Self {
        self.deployment_log = Some(log);
        self
    }

    /// Run a unit with the specified command
    pub async fn run(
        &self,
        unit: &Unit,
        command: Vec<String>,
        auto_approve: bool,
    ) -> AppResult<RunResult> {
        // Create runner based on unit's runner type
        let runner = self
            .runner_factory
            .create_runner(unit.manifest.runner_type.as_str())?;

        // Initialize runner
        runner.init().await?;

        // Build params
        let params = RunParams::new(unit.source_path.as_deref().unwrap_or("."))
            .with_commands(command.clone())
            .with_auto_approve(auto_approve);

        // Execute
        let result = runner.execute(unit, &params).await?;

        // Log deployment
        if let Some(log) = &self.deployment_log {
            let entry = DeploymentLogEntry {
                unit_name: unit.name.clone(),
                org: unit.org.clone(),
                dimensions: unit.dimensions.iter().map(|d| d.key()).collect(),
                command: command.join(" "),
                exit_code: result.exit_code,
                timestamp: SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0),
                duration_ms: result.duration_ms,
                git_shas: HashMap::new(), // TODO: Add git SHA support
                metadata: HashMap::new(),
            };
            log.save(&entry).await?;
        }

        Ok(result)
    }

    /// Get available runner types
    pub fn available_runners(&self) -> Vec<&str> {
        self.runner_factory.available_runners()
    }
}

