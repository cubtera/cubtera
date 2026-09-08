//! Repository factory

use cubtera_config::Config;
use cubtera_core::ports::{
    DeploymentLogRepository, InventoryRepository, UnitRepository, UnitStateRepository,
};
use cubtera_domain::DimHierarchy;
use cubtera_store::SqliteStore;
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
    /// Inventory is always FS-backed, rooted at `inventory_path` (MongoDB
    /// support - `CUBTERA_DB`, `im sync*` - was removed in P2; see
    /// docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9).
    ///
    /// The deployment log and unit state ports are always backed by one
    /// shared `SqliteStore` opened at `config.store_path` - no more
    /// FS-jsonl/FS-json/Mongo three-way choice.
    pub async fn from_config(config: &Config) -> Result<Self, String> {
        let unit_repo = crate::fs::FsUnitRepository::new(config.units_path.clone());

        let store = Arc::new(
            SqliteStore::open(&config.store_path)
                .map_err(|e| format!("failed to open store at {:?}: {e}", config.store_path))?,
        );
        let deployment_log: Arc<dyn DeploymentLogRepository> = Arc::new(
            crate::sqlite::SqliteDeploymentLogRepository::new(store.clone()),
        );
        let unit_state: Arc<dyn UnitStateRepository> =
            Arc::new(crate::sqlite::SqliteUnitStateRepository::new(store));

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

    /// Build a [`DimHierarchy`] from the config's `dim_relations`
    pub fn hierarchy(config: &Config) -> DimHierarchy {
        DimHierarchy::new(config.dim_relations.clone())
    }
}
