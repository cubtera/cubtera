//! Unit service

use crate::error::{AppError, AppResult};
use crate::ports::{DimensionRepository, UnitRepository};
use cubtera_domain::{DimensionRef, Manifest, Unit};
use serde_json::Value;
use std::collections::HashMap;
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

        // Parse dimension keys and load dimension data
        let mut dims: Vec<DimensionRef> = Vec::new();
        let mut dim_data: HashMap<String, Value> = HashMap::new();

        for key in dimension_keys {
            let dim_ref = DimensionRef::parse(key).ok_or_else(|| {
                AppError::validation(format!("Invalid dimension format: {}", key))
            })?;

            // Load dimension data from repository
            if let Some(dimension) = self
                .dimension_repository
                .find_by_name(org, &dim_ref.dim_type, &dim_ref.name)
                .await?
            {
                // Convert HashMap<String, domain::Value> to serde_json::Value
                let data_value = cubtera_domain::Value::hashmap_to_json(&dimension.data);
                dim_data.insert(dim_ref.dim_type.as_str().to_string(), data_value);
            }

            dims.push(dim_ref);
        }

        // Create unit
        let mut unit = Unit::new(unit_name, org, manifest);

        // Add dimensions
        for dim_ref in dims {
            unit = unit.with_dimension(dim_ref);
        }

        // Add dimension data
        unit = unit.with_all_dimension_data(dim_data);

        // Set unit path
        if let Some(path) = self.unit_repository.get_unit_path(org, unit_name).await? {
            unit = unit.with_unit_path(path);
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

