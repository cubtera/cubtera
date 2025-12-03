//! Repository factory

use cubtera_config::{Config, StorageBackend};
use cubtera_core::ports::{DimensionRepository, UnitRepository};
use std::sync::Arc;

/// Create repositories based on configuration
pub struct Repositories {
    pub dimensions: Arc<dyn DimensionRepository>,
    pub units: Arc<dyn UnitRepository>,
}

impl Repositories {
    /// Create repositories from config
    pub fn from_config(config: &Config) -> Result<Self, String> {
        match &config.storage {
            #[cfg(feature = "fs")]
            StorageBackend::Fs { path } => {
                let dim_repo = crate::fs::FsDimensionRepository::new(
                    path.clone(),
                    config.org.clone(),
                );
                let unit_repo = crate::fs::FsUnitRepository::new(
                    config.units_path.clone(),
                    config.org.clone(),
                );
                Ok(Self {
                    dimensions: Arc::new(dim_repo),
                    units: Arc::new(unit_repo),
                })
            }
            #[cfg(feature = "mongodb")]
            StorageBackend::MongoDB { connection_string } => {
                // MongoDB implementation would go here
                Err("MongoDB not yet implemented".to_string())
            }
            StorageBackend::Postgres { .. } => {
                Err("PostgreSQL not yet implemented".to_string())
            }
            #[allow(unreachable_patterns)]
            _ => Err("No matching storage backend feature enabled".to_string()),
        }
    }
}

