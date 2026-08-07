//! Repository ports (interfaces)
//!
//! Traits for data access. Implementations live in cubtera-persistence.

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::{Dimension, DimType, Manifest};

/// Repository for dimension data
#[async_trait]
pub trait DimensionRepository: Send + Sync {
    /// Find a dimension by type and name
    async fn find_by_name(
        &self,
        org: &str,
        dim_type: &DimType,
        name: &str,
    ) -> AppResult<Option<Dimension>>;

    /// Find all dimensions of a given type
    async fn find_all(&self, org: &str, dim_type: &DimType) -> AppResult<Vec<Dimension>>;

    /// Find all dimension names of a given type
    async fn find_names(&self, org: &str, dim_type: &DimType) -> AppResult<Vec<String>>;

    /// Find default dimension for a type
    async fn find_defaults(
        &self,
        org: &str,
        dim_type: &DimType,
    ) -> AppResult<Option<Dimension>>;

    /// Find children of a dimension
    async fn find_children(
        &self,
        org: &str,
        parent_type: &DimType,
        parent_name: &str,
    ) -> AppResult<Vec<Dimension>>;

    /// Find parent of a dimension
    async fn find_parent(
        &self,
        org: &str,
        dim_type: &DimType,
        name: &str,
    ) -> AppResult<Option<Dimension>>;

    /// Get all available dimension types
    async fn get_dim_types(&self, org: &str) -> AppResult<Vec<String>>;

    /// Get all available organizations
    async fn get_orgs(&self) -> AppResult<Vec<String>>;

    /// Save a dimension
    async fn save(&self, org: &str, dimension: &Dimension) -> AppResult<()>;

    /// Delete a dimension
    async fn delete(&self, org: &str, dim_type: &DimType, name: &str) -> AppResult<()>;
}

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

/// Repository factory trait for creating repositories based on configuration
pub trait RepositoryFactory: Send + Sync {
    /// Create a dimension repository
    fn dimension_repository(&self) -> Box<dyn DimensionRepository>;

    /// Create a unit repository
    fn unit_repository(&self) -> Box<dyn UnitRepository>;
}

