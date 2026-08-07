//! Inventory port (interface)
//!
//! `InventoryRepository` is deliberately "dumb": it only knows how to turn
//! whatever the backing store looks like (files on disk, MongoDB documents,
//! ...) into a [`RawDimension`] - sections keyed by their logical name plus
//! attached includes. It performs **no** business logic: no defaults
//! gap-filling, no parent resolution, no `meta` wrapping beyond what the
//! storage's own naming convention already encodes.
//!
//! All of that assembly lives in the domain (`Dimension::assemble`) and is
//! orchestrated by [`crate::services::DimensionService`]. This split means a
//! future MongoDB adapter only has to implement this port - it gets the same
//! business rules "for free" through the service, instead of re-implementing
//! them (which is exactly what went wrong in the v1 adapters).

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::RawDimension;

/// Repository for raw dimension records (no business logic applied)
#[async_trait]
pub trait InventoryRepository: Send + Sync {
    /// Fetch the raw record for a dimension by type and name
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawDimension>>;

    /// Fetch the raw defaults record for a dimension type (".default" record)
    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>>;

    /// Fetch the raw JSON-schema record for a dimension type (".schema"
    /// record; its "meta" section is the schema that validates a
    /// dimension's own "meta" section)
    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>>;

    /// List all dimension names of a given type (excludes reserved names like
    /// ".default"/".schema")
    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>>;

    /// List all available dimension types for an org
    async fn list_types(&self, org: &str) -> AppResult<Vec<String>>;

    /// List all available organizations
    async fn list_orgs(&self) -> AppResult<Vec<String>>;

    /// Persist a raw dimension record
    async fn save_raw(&self, org: &str, dim_type: &str, raw: &RawDimension) -> AppResult<()>;

    /// Delete a dimension record
    async fn delete_raw(&self, org: &str, dim_type: &str, name: &str) -> AppResult<()>;
}
