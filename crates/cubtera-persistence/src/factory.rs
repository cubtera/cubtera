//! Repository factory

use cubtera_config::Config;
use cubtera_core::ports::{
    DeploymentLogRepository, InventoryRepository, UnitRepository, UnitStateRepository,
};
use cubtera_domain::DimHierarchy;
use std::sync::Arc;

/// Create repositories based on configuration
pub struct Repositories {
    pub inventory: Arc<dyn InventoryRepository>,
    pub units: Arc<dyn UnitRepository>,
    pub deployment_log: Arc<dyn DeploymentLogRepository>,
    pub unit_state: Arc<dyn UnitStateRepository>,
}

impl Repositories {
    /// Create repositories from config.
    ///
    /// Inventory backend selection mirrors v1: `CUBTERA_DB` set (surfaced as
    /// [`Config::mongodb_connection_string`]) means Mongo, otherwise FS via
    /// `inventory_path`. Enabling the `mongodb` connection string without
    /// the `mongodb` feature compiled in is a hard error rather than a
    /// silent FS fallback.
    ///
    /// Deployment log backend selection is independent: `config.toml`'s
    /// `[deploymentLog]` table set means Mongo, otherwise FS-jsonl rooted at
    /// `deployment_log_path`.
    pub async fn from_config(config: &Config) -> Result<Self, String> {
        let unit_repo = crate::fs::FsUnitRepository::new(config.units_path.clone());
        let deployment_log = Self::deployment_log_from_config(config).await?;
        let unit_state = Self::unit_state_from_config(config).await?;

        #[cfg(feature = "mongodb")]
        if let Some(connection_string) = &config.mongodb_connection_string {
            let inventory = crate::mongodb::MongoInventoryRepository::new(
                connection_string,
                config.file_name_separator.clone(),
            )
            .await?;
            return Ok(Self {
                inventory: Arc::new(inventory),
                units: Arc::new(unit_repo),
                deployment_log,
                unit_state,
            });
        }

        #[cfg(not(feature = "mongodb"))]
        if config.mongodb_connection_string.is_some() {
            return Err(
                "CUBTERA_DB is set but this build has no `mongodb` feature enabled".to_string(),
            );
        }

        #[cfg(feature = "fs")]
        {
            let inventory = crate::fs::FsInventoryRepository::new(config.inventory_path.clone())
                .with_separator(config.file_name_separator.clone());
            Ok(Self {
                inventory: Arc::new(inventory),
                units: Arc::new(unit_repo),
                deployment_log,
                unit_state,
            })
        }

        #[cfg(not(feature = "fs"))]
        {
            Err("No matching storage backend feature enabled".to_string())
        }
    }

    async fn deployment_log_from_config(
        config: &Config,
    ) -> Result<Arc<dyn DeploymentLogRepository>, String> {
        #[cfg(feature = "mongodb")]
        if let Some(dlog_config) = &config.deployment_log {
            let repo = crate::mongodb::MongoDeploymentLogRepository::new(
                &dlog_config.connection_string,
                &dlog_config.database,
                &dlog_config.collection,
            )
            .await?;
            return Ok(Arc::new(repo));
        }

        #[cfg(not(feature = "mongodb"))]
        if config.deployment_log.is_some() {
            return Err(
                "[deploymentLog] is set but this build has no `mongodb` feature enabled"
                    .to_string(),
            );
        }

        Ok(Arc::new(crate::fs::FsDeploymentLogRepository::new(
            config.deployment_log_path.clone(),
        )))
    }

    async fn unit_state_from_config(
        config: &Config,
    ) -> Result<Arc<dyn UnitStateRepository>, String> {
        #[cfg(feature = "mongodb")]
        if let Some(unit_state_config) = &config.unit_state {
            let repo = crate::mongodb::MongoUnitStateRepository::new(
                &unit_state_config.connection_string,
                &unit_state_config.database,
                &unit_state_config.collection,
            )
            .await?;
            return Ok(Arc::new(repo));
        }

        #[cfg(not(feature = "mongodb"))]
        if config.unit_state.is_some() {
            return Err(
                "[unitState] is set but this build has no `mongodb` feature enabled".to_string(),
            );
        }

        Ok(Arc::new(crate::fs::FsUnitStateRepository::new(
            config.unit_state_path.clone(),
        )))
    }

    /// Build a [`DimHierarchy`] from the config's `dim_relations`
    pub fn hierarchy(config: &Config) -> DimHierarchy {
        DimHierarchy::new(config.dim_relations.clone())
    }
}
