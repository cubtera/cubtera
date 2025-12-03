//! Unit service

use crate::error::{AppError, AppResult};
use crate::ports::{DimensionRepository, UnitRepository};
use cubtera_domain::{DimType, DimensionRef, Manifest, Unit};
use std::sync::Arc;

/// Service for unit operations
pub struct UnitService {
    unit_repository: Arc<dyn UnitRepository>,
    dimension_repository: Arc<dyn DimensionRepository>,
}

impl UnitService {
    /// Create a new unit service
    pub fn new(
        unit_repository: Arc<dyn UnitRepository>,
        dimension_repository: Arc<dyn DimensionRepository>,
    ) -> Self {
        Self {
            unit_repository,
            dimension_repository,
        }
    }

    /// Build a unit with resolved dimensions
    pub async fn build_unit(
        &self,
        org: &str,
        unit_name: &str,
        dimension_keys: &[String],
    ) -> AppResult<Unit> {
        // Load manifest
        let manifest = self
            .unit_repository
            .find_manifest(org, unit_name)
            .await?
            .ok_or_else(|| AppError::not_found("unit", unit_name))?;

        // Parse dimension keys
        let mut dims: Vec<DimensionRef> = Vec::new();
        for key in dimension_keys {
            let dim_ref = DimensionRef::parse(key).ok_or_else(|| {
                AppError::validation(format!("Invalid dimension format: {}", key))
            })?;
            dims.push(dim_ref);
        }

        // Create unit
        let mut unit = Unit::new(unit_name, org, manifest);

        // Add dimensions
        for dim_ref in dims {
            unit = unit.with_dimension(dim_ref);
        }

        // Set source path
        if let Some(path) = self.unit_repository.get_unit_path(org, unit_name).await? {
            unit = unit.with_source_path(path);
        }

        // Validate required dimensions
        if !unit.has_all_required_dimensions() {
            let missing = unit.missing_dimensions();
            return Err(AppError::validation(format!(
                "Missing required dimensions: {}",
                missing.join(", ")
            )));
        }

        Ok(unit)
    }

    /// List all available units
    pub async fn list_units(&self, org: &str) -> AppResult<Vec<String>> {
        self.unit_repository.list_units(org).await
    }

    /// Get unit manifest
    pub async fn get_manifest(&self, org: &str, unit_name: &str) -> AppResult<Manifest> {
        self.unit_repository
            .find_manifest(org, unit_name)
            .await?
            .ok_or_else(|| AppError::not_found("unit", unit_name))
    }
}

