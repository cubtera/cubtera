//! Repository ports (interfaces)
//!
//! Traits for data access. Implementations live in cubtera-persistence.
//! See [`crate::ports::InventoryRepository`] for dimension data access.

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::Manifest;

/// Repository for unit manifests
#[async_trait]
pub trait UnitRepository: Send + Sync {
    /// Find a unit manifest by name
    async fn find_manifest(&self, org: &str, unit_name: &str) -> AppResult<Option<Manifest>>;

    /// Get the unit source path
    async fn get_unit_path(&self, org: &str, unit_name: &str) -> AppResult<Option<String>>;

    /// List all available units
    async fn list_units(&self, org: &str) -> AppResult<Vec<String>>;
}
