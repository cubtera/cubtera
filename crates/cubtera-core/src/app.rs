//! Application composition root
//!
//! The App struct wires all dependencies together.

use crate::ports::{
    CopyConfig, DeploymentLogRepository, InventoryRepository, ProcessRunner, RunnerFactory,
    UnitRepository, Workspace,
};
use crate::services::{DimensionService, RunService, UnitService};
use cubtera_domain::DimHierarchy;
use std::path::PathBuf;
use std::sync::Arc;

/// Application instance with all services wired
pub struct App {
    /// Dimension service
    pub dimensions: Arc<DimensionService>,
    /// Unit service
    pub units: UnitService,
    /// Run service (runner pipeline)
    pub runners: RunService,
}

impl App {
    /// Create a new application with the given dependencies
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        inventory_repository: Arc<dyn InventoryRepository>,
        dim_hierarchy: DimHierarchy,
        unit_repository: Arc<dyn UnitRepository>,
        runner_factory: Arc<dyn RunnerFactory>,
        workspace: Arc<dyn Workspace>,
        process: Arc<dyn ProcessRunner>,
        copy_config: CopyConfig,
        deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
    ) -> Self {
        let dimensions = Arc::new(DimensionService::new(inventory_repository, dim_hierarchy));
        let units = UnitService::new(unit_repository, dimensions.clone());
        let mut runners = RunService::new(runner_factory, workspace, process, copy_config);

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
    inventory_repository: Option<Arc<dyn InventoryRepository>>,
    dim_hierarchy: Option<DimHierarchy>,
    unit_repository: Option<Arc<dyn UnitRepository>>,
    runner_factory: Option<Arc<dyn RunnerFactory>>,
    workspace: Option<Arc<dyn Workspace>>,
    process: Option<Arc<dyn ProcessRunner>>,
    deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
    copy_config: Option<CopyConfig>,
}

impl AppBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            inventory_repository: None,
            dim_hierarchy: None,
            unit_repository: None,
            runner_factory: None,
            workspace: None,
            process: None,
            deployment_log: None,
            copy_config: None,
        }
    }

    /// Set inventory repository
    pub fn inventory_repository(mut self, repo: Arc<dyn InventoryRepository>) -> Self {
        self.inventory_repository = Some(repo);
        self
    }

    /// Set dimension hierarchy (dim_relations)
    pub fn dim_hierarchy(mut self, hierarchy: DimHierarchy) -> Self {
        self.dim_hierarchy = Some(hierarchy);
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

    /// Set workspace (materialization plan executor)
    pub fn workspace(mut self, workspace: Arc<dyn Workspace>) -> Self {
        self.workspace = Some(workspace);
        self
    }

    /// Set process runner (spawns OS processes for strategies/hooks)
    pub fn process(mut self, process: Arc<dyn ProcessRunner>) -> Self {
        self.process = Some(process);
        self
    }

    /// Set deployment log repository
    pub fn deployment_log(mut self, log: Arc<dyn DeploymentLogRepository>) -> Self {
        self.deployment_log = Some(log);
        self
    }

    /// Set copy config
    pub fn copy_config(mut self, config: CopyConfig) -> Self {
        self.copy_config = Some(config);
        self
    }

    /// Build the App
    pub fn build(self) -> Result<App, &'static str> {
        let inventory_repository = self
            .inventory_repository
            .ok_or("inventory_repository is required")?;
        let unit_repository = self.unit_repository.ok_or("unit_repository is required")?;
        let runner_factory = self.runner_factory.ok_or("runner_factory is required")?;
        let workspace = self.workspace.ok_or("workspace is required")?;
        let process = self.process.ok_or("process is required")?;

        // Use default copy_config if not provided
        let copy_config = self.copy_config.unwrap_or_else(|| CopyConfig {
            modules_path: PathBuf::from("modules"),
            plugins_path: PathBuf::from("plugins"),
            always_copy_files: false,
            clean_cache: false,
        });

        Ok(App::new(
            inventory_repository,
            self.dim_hierarchy.unwrap_or_default(),
            unit_repository,
            runner_factory,
            workspace,
            process,
            copy_config,
            self.deployment_log,
        ))
    }
}

impl Default for AppBuilder {
    fn default() -> Self {
        Self::new()
    }
}
