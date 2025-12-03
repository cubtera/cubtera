//! Application composition root
//!
//! The App struct wires all dependencies together.

use crate::ports::{DeploymentLogRepository, DimensionRepository, RunnerFactory, UnitRepository};
use crate::services::{DimensionService, RunnerService, UnitService};
use std::sync::Arc;

/// Application instance with all services wired
pub struct App {
    /// Dimension service
    pub dimensions: DimensionService,
    /// Unit service
    pub units: UnitService,
    /// Runner service
    pub runners: RunnerService,
}

impl App {
    /// Create a new application with the given dependencies
    pub fn new(
        dimension_repository: Arc<dyn DimensionRepository>,
        unit_repository: Arc<dyn UnitRepository>,
        runner_factory: Arc<dyn RunnerFactory>,
        deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
    ) -> Self {
        let dimensions = DimensionService::new(dimension_repository.clone());
        let units = UnitService::new(unit_repository, dimension_repository);
        let mut runners = RunnerService::new(runner_factory);

        if let Some(log) = deployment_log {
            runners = runners.with_deployment_log(log);
        }

        Self {
            dimensions,
            units,
            runners,
        }
    }
}

/// Builder for App
pub struct AppBuilder {
    dimension_repository: Option<Arc<dyn DimensionRepository>>,
    unit_repository: Option<Arc<dyn UnitRepository>>,
    runner_factory: Option<Arc<dyn RunnerFactory>>,
    deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
}

impl AppBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            dimension_repository: None,
            unit_repository: None,
            runner_factory: None,
            deployment_log: None,
        }
    }

    /// Set dimension repository
    pub fn dimension_repository(mut self, repo: Arc<dyn DimensionRepository>) -> Self {
        self.dimension_repository = Some(repo);
        self
    }

    /// Set unit repository
    pub fn unit_repository(mut self, repo: Arc<dyn UnitRepository>) -> Self {
        self.unit_repository = Some(repo);
        self
    }

    /// Set runner factory
    pub fn runner_factory(mut self, factory: Arc<dyn RunnerFactory>) -> Self {
        self.runner_factory = Some(factory);
        self
    }

    /// Set deployment log repository
    pub fn deployment_log(mut self, log: Arc<dyn DeploymentLogRepository>) -> Self {
        self.deployment_log = Some(log);
        self
    }

    /// Build the App
    pub fn build(self) -> Result<App, &'static str> {
        let dimension_repository = self
            .dimension_repository
            .ok_or("dimension_repository is required")?;
        let unit_repository = self.unit_repository.ok_or("unit_repository is required")?;
        let runner_factory = self.runner_factory.ok_or("runner_factory is required")?;

        Ok(App::new(
            dimension_repository,
            unit_repository,
            runner_factory,
            self.deployment_log,
        ))
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}

