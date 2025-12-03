//! Dimension service

use crate::error::{AppError, AppResult};
use crate::ports::DimensionRepository;
use cubtera_domain::{DimType, Dimension};
use std::sync::Arc;

/// Service for dimension operations
pub struct DimensionService {
    repository: Arc<dyn DimensionRepository>,
}

impl DimensionService {
    /// Create a new dimension service
    pub fn new(repository: Arc<dyn DimensionRepository>) -> Self {
        Self { repository }
    }

    /// Get all dimension names by type
    pub async fn get_all_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        let dim_type = DimType::new(dim_type);
        self.repository.find_names(org, &dim_type).await
    }

    /// Get all dimensions by type with data
    pub async fn get_all(&self, org: &str, dim_type: &str) -> AppResult<Vec<Dimension>> {
        let dim_type = DimType::new(dim_type);
        self.repository.find_all(org, &dim_type).await
    }

    /// Get dimension by name
    pub async fn get_by_name(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Dimension> {
        let dim_type = DimType::new(dim_type);
        self.repository
            .find_by_name(org, &dim_type, name)
            .await?
            .ok_or_else(|| AppError::not_found("dimension", format!("{}:{}", dim_type, name)))
    }

    /// Get default dimension for type
    pub async fn get_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<Dimension>> {
        let dim_type = DimType::new(dim_type);
        self.repository.find_defaults(org, &dim_type).await
    }

    /// Get children of a dimension
    pub async fn get_children(
        &self,
        org: &str,
        parent_type: &str,
        parent_name: &str,
    ) -> AppResult<Vec<Dimension>> {
        let parent_type = DimType::new(parent_type);
        self.repository
            .find_children(org, &parent_type, parent_name)
            .await
    }

    /// Get parent of a dimension
    pub async fn get_parent(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<Dimension>> {
        let dim_type = DimType::new(dim_type);
        self.repository.find_parent(org, &dim_type, name).await
    }

    /// Get all dimension types
    pub async fn get_types(&self, org: &str) -> AppResult<Vec<String>> {
        self.repository.get_dim_types(org).await
    }

    /// Get all organizations
    pub async fn get_orgs(&self) -> AppResult<Vec<String>> {
        self.repository.get_orgs().await
    }

    /// Validate a dimension exists
    pub async fn validate(&self, org: &str, dim_type: &str, name: &str) -> AppResult<bool> {
        let dim_type = DimType::new(dim_type);
        let dim = self.repository.find_by_name(org, &dim_type, name).await?;
        Ok(dim.is_some())
    }
}

