//! Unit service

use crate::error::{AppError, AppResult};
use crate::ports::UnitRepository;
use crate::services::DimensionService;
use cubtera_domain::{
    AccessPolicy, DimensionAccessContext, DimensionRef, IncludeEntry, Manifest, Unit,
};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Service for unit operations
pub struct UnitService {
    unit_repository: Arc<dyn UnitRepository>,
    dimensions: Arc<DimensionService>,
}

impl UnitService {
    /// Create a new unit service
    pub fn new(
        unit_repository: Arc<dyn UnitRepository>,
        dimensions: Arc<DimensionService>,
    ) -> Self {
        Self {
            unit_repository,
            dimensions,
        }
    }

    /// Build a unit with resolved dimensions
    pub async fn build_unit(
        &self,
        org: &str,
        unit_name: &str,
        dimension_keys: &[String],
    ) -> AppResult<Unit> {
        self.build_unit_with_extensions(org, unit_name, dimension_keys, &[])
            .await
    }

    /// Build a unit with resolved dimensions and extensions ("type:name"
    /// pairs used to run the same unit/dimensions with a different state,
    /// e.g. `-e index:0` for a sharded deployment - see `Unit::with_extensions`).
    pub async fn build_unit_with_extensions(
        &self,
        org: &str,
        unit_name: &str,
        dimension_keys: &[String],
        extensions: &[String],
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
        // Full "type:name" chain of every resolved dimension and its
        // ancestors, so allowList/denyList can gate on a parent dimension.
        let mut dims_tree: HashSet<String> = HashSet::new();
        let mut access_contexts: Vec<DimensionAccessContext> = Vec::new();
        let mut includes: Vec<IncludeEntry> = Vec::new();

        for key in dimension_keys {
            let dim_ref = DimensionRef::parse(key).ok_or_else(|| {
                AppError::validation(format!("Invalid dimension format: {}", key))
            })?;

            // Load fully assembled dimension data (defaults + parent chain already applied)
            let dimension = self
                .dimensions
                .get_by_name(org, dim_ref.dim_type.as_str(), &dim_ref.name)
                .await?;
            dim_data.insert(dim_ref.dim_type.as_str().to_string(), dimension.to_json());
            includes.extend(dimension.includes.iter().cloned());

            dims_tree.extend(dimension.key_path.iter().cloned());
            let affinity_tags = dimension
                .meta()
                .and_then(|m| m.get("affinity_tags"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            access_contexts.push(DimensionAccessContext {
                key: dimension.key(),
                affinity_tags,
            });

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
        unit = unit.with_includes(includes);
        if !extensions.is_empty() {
            unit = unit.with_extensions(extensions.to_vec());
        }

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

        // Access policy: allowList/denyList/affinityTags. See
        // `cubtera_domain::AccessPolicy` for why this is data, not `exit(0)`.
        match AccessPolicy::evaluate(&unit.manifest, &dims_tree, &access_contexts) {
            cubtera_domain::AccessDecision::Allowed => {}
            cubtera_domain::AccessDecision::Denied { reason } => {
                return Err(AppError::access_denied(format!(
                    "unit '{}' denied for dimensions {:?}: {}",
                    unit_name, dims_tree, reason
                )));
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::InventoryRepository;
    use async_trait::async_trait;
    use cubtera_domain::{DimHierarchy, RawDimension};
    use serde_json::json;

    struct FakeInventory {
        // dim_type -> name -> raw
        data: HashMap<String, HashMap<String, RawDimension>>,
    }

    #[async_trait]
    impl InventoryRepository for FakeInventory {
        async fn get_raw(
            &self,
            _org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Option<RawDimension>> {
            Ok(self.data.get(dim_type).and_then(|n| n.get(name)).cloned())
        }
        async fn get_raw_defaults(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawDimension>> {
            Ok(None)
        }
        async fn get_raw_schema(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawDimension>> {
            Ok(None)
        }
        async fn list_names(&self, _org: &str, dim_type: &str) -> AppResult<Vec<String>> {
            Ok(self
                .data
                .get(dim_type)
                .map(|n| n.keys().cloned().collect())
                .unwrap_or_default())
        }
        async fn list_types(&self, _org: &str) -> AppResult<Vec<String>> {
            Ok(self.data.keys().cloned().collect())
        }
        async fn list_orgs(&self) -> AppResult<Vec<String>> {
            Ok(vec!["cubtera".to_string()])
        }
        async fn save_raw(
            &self,
            _org: &str,
            _dim_type: &str,
            _raw: &RawDimension,
        ) -> AppResult<()> {
            Ok(())
        }
        async fn delete_raw(&self, _org: &str, _dim_type: &str, _name: &str) -> AppResult<()> {
            Ok(())
        }
    }

    struct FakeUnits {
        manifest: Manifest,
    }

    #[async_trait]
    impl UnitRepository for FakeUnits {
        async fn find_manifest(&self, _org: &str, _unit_name: &str) -> AppResult<Option<Manifest>> {
            Ok(Some(self.manifest.clone()))
        }
        async fn get_unit_path(&self, _org: &str, _unit_name: &str) -> AppResult<Option<String>> {
            Ok(Some("/units/network".to_string()))
        }
        async fn list_units(&self, _org: &str) -> AppResult<Vec<String>> {
            Ok(vec!["network".to_string()])
        }
    }

    fn dimension_service(env_meta: serde_json::Value) -> Arc<DimensionService> {
        let mut env = HashMap::new();
        env.insert(
            "prod".to_string(),
            RawDimension::new("prod").with_section("meta", env_meta),
        );
        let mut data = HashMap::new();
        data.insert("env".to_string(), env);

        Arc::new(DimensionService::new(
            Arc::new(FakeInventory { data }),
            DimHierarchy::new(vec!["env"]),
        ))
    }

    fn unit_service(manifest: Manifest, env_meta: serde_json::Value) -> UnitService {
        UnitService::new(
            Arc::new(FakeUnits { manifest }),
            dimension_service(env_meta),
        )
    }

    #[tokio::test]
    async fn build_unit_succeeds_with_no_policy_restrictions() {
        let manifest = Manifest::new(vec!["env".to_string()], "tf");
        let service = unit_service(manifest, json!({}));

        let unit = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();
        assert_eq!(unit.name, "network");
        assert_eq!(unit.dimensions[0].key(), "env:prod");
    }

    #[tokio::test]
    async fn build_unit_denied_when_dimension_not_in_allow_list() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.allow_list = Some(vec!["env:staging".to_string()]);
        let service = unit_service(manifest, json!({}));

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::AccessDenied(_))));
    }

    #[tokio::test]
    async fn build_unit_denied_when_dimension_missing_affinity_tag() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.affinity_tags = Some(vec!["critical".to_string()]);
        let service = unit_service(manifest, json!({"affinity_tags": ["core"]}));

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::AccessDenied(_))));
    }

    #[tokio::test]
    async fn build_unit_allowed_when_affinity_tags_overlap() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.affinity_tags = Some(vec!["core".to_string()]);
        let service = unit_service(manifest, json!({"affinity_tags": ["core"]}));

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn build_unit_fails_validation_for_missing_required_dimension() {
        let manifest = Manifest::new(vec!["env".to_string(), "dc".to_string()], "tf");
        let service = unit_service(manifest, json!({}));

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::Validation(_))));
    }
}
