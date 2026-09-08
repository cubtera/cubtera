//! Unit service

use crate::error::{AppError, AppResult};
use crate::ports::{UnitRepository, UnitStateRepository};
use crate::services::DimensionService;
use cubtera_domain::{
    project_state_key, AccessPolicy, DimensionAccessContext, DimensionRef, IncludeEntry, Manifest,
    Unit, UnitStateKey,
};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;

/// Service for unit operations
pub struct UnitService {
    unit_repository: Arc<dyn UnitRepository>,
    dimensions: Arc<DimensionService>,
    /// `None` means `[inputs.<alias>]` resolution is unavailable - a
    /// manifest that declares inputs anyway is a hard configuration error,
    /// never a silent skip (see `Self::resolve_inputs`).
    unit_state: Option<Arc<dyn UnitStateRepository>>,
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
            unit_state: None,
        }
    }

    /// Enable `[inputs.<alias>]` resolution against a unit state store.
    pub fn with_unit_state(mut self, unit_state: Arc<dyn UnitStateRepository>) -> Self {
        self.unit_state = Some(unit_state);
        self
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
        // v3 seam (docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4):
        // `org`/`unit_name` end up in `Unit::calculate_temp_folder`'s
        // `base_path.join(org).join(name)` completely unvalidated, and
        // `extensions` used to skip `DimensionRef` parsing entirely and go
        // straight into `Unit.extensions`, which the same temp-folder
        // builder joins onto the path one entry at a time. Validate all
        // three up front - once, here, since every entry point (CLI `run`,
        // future API/server run endpoints) goes through this method - so a
        // crafted `-e '../../../etc'` or `-u '../../etc'` is a validation
        // error, not a write outside `tempFolderPath`.
        cubtera_kernel::Ident::parse(org)
            .map_err(|e| AppError::validation(format!("invalid org {org:?}: {e}")))?;
        cubtera_kernel::Ident::parse(unit_name)
            .map_err(|e| AppError::validation(format!("invalid unit name {unit_name:?}: {e}")))?;
        for ext in extensions {
            cubtera_kernel::DimRef::parse(ext)
                .map_err(|e| AppError::validation(format!("invalid extension {ext:?}: {e}")))?;
        }

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
        // Sorted for deterministic `--dry-run` output and error messages;
        // `project_state_key` only filters by prefix, so order doesn't
        // affect correctness.
        let mut dim_key_path: Vec<String> = dims_tree.iter().cloned().collect();
        dim_key_path.sort();
        unit = unit.with_dim_key_path(dim_key_path);
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

        unit = self.resolve_inputs(org, unit_name, unit).await?;

        Ok(unit)
    }

    /// Resolve every `[inputs.<alias>]` entry in `unit`'s manifest into a
    /// producer's published outputs, projecting the producer's required
    /// dimensions onto `unit.dim_key_path` when the manifest doesn't name
    /// them explicitly (see `cubtera_domain::project_state_key`).
    ///
    /// A manifest with `[inputs.*]` but no unit state store configured is a
    /// hard error - never a silent "no inputs resolved". Same for a missing
    /// required input.
    async fn resolve_inputs(&self, org: &str, unit_name: &str, unit: Unit) -> AppResult<Unit> {
        if unit.manifest.inputs.is_empty() {
            return Ok(unit);
        }

        let store = self.unit_state.as_ref().ok_or_else(|| {
            AppError::config(format!(
                "unit '{unit_name}' declares [inputs] but no unit state store is configured \
                 (set unitStatePath or [unitState] in config.toml)"
            ))
        })?;

        let mut resolved_inputs: BTreeMap<String, Value> = BTreeMap::new();
        for (alias, spec) in &unit.manifest.inputs {
            let dims = match &spec.dims {
                Some(explicit) => explicit.clone(),
                None => {
                    let producer_manifest = self
                        .unit_repository
                        .find_manifest(org, &spec.unit)
                        .await?
                        .ok_or_else(|| AppError::not_found("unit", spec.unit.clone()))?;
                    project_state_key(&unit.dim_key_path, &producer_manifest.dimensions)?
                }
            };
            let ext = spec.ext.clone().unwrap_or_default();
            let key = UnitStateKey::new(org, spec.unit.clone(), dims, ext);

            match store.get(&key).await? {
                Some(record) => {
                    resolved_inputs.insert(alias.clone(), record.outputs);
                }
                None if spec.is_required() => {
                    return Err(AppError::not_found(
                        "unit state",
                        format!("{} (projected key: {})", spec.unit, key.canonical()),
                    ));
                }
                None => {}
            }
        }

        Ok(unit.with_resolved_inputs(resolved_inputs))
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
        manifests: HashMap<String, Manifest>,
    }

    #[async_trait]
    impl UnitRepository for FakeUnits {
        async fn find_manifest(&self, _org: &str, unit_name: &str) -> AppResult<Option<Manifest>> {
            Ok(self.manifests.get(unit_name).cloned())
        }
        async fn get_unit_path(&self, _org: &str, _unit_name: &str) -> AppResult<Option<String>> {
            Ok(Some("/units/network".to_string()))
        }
        async fn list_units(&self, _org: &str) -> AppResult<Vec<String>> {
            Ok(self.manifests.keys().cloned().collect())
        }
    }

    #[derive(Default)]
    struct FakeUnitState {
        records: std::sync::Mutex<
            HashMap<cubtera_domain::UnitStateKey, cubtera_domain::UnitStateRecord>,
        >,
    }

    impl FakeUnitState {
        fn with_record(record: cubtera_domain::UnitStateRecord) -> Self {
            let state = Self::default();
            state.records.lock().unwrap().insert(record.key(), record);
            state
        }
    }

    #[async_trait]
    impl crate::ports::UnitStateRepository for FakeUnitState {
        async fn get(
            &self,
            key: &cubtera_domain::UnitStateKey,
        ) -> AppResult<Option<cubtera_domain::UnitStateRecord>> {
            Ok(self.records.lock().unwrap().get(key).cloned())
        }
        async fn put(&self, record: &cubtera_domain::UnitStateRecord) -> AppResult<()> {
            self.records
                .lock()
                .unwrap()
                .insert(record.key(), record.clone());
            Ok(())
        }
        async fn delete(&self, key: &cubtera_domain::UnitStateKey) -> AppResult<()> {
            self.records.lock().unwrap().remove(key);
            Ok(())
        }
        async fn list(
            &self,
            org: &str,
            unit: &str,
        ) -> AppResult<Vec<cubtera_domain::UnitStateRecord>> {
            Ok(self
                .records
                .lock()
                .unwrap()
                .values()
                .filter(|r| r.org == org && r.unit == unit)
                .cloned()
                .collect())
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
        let mut manifests = HashMap::new();
        manifests.insert("network".to_string(), manifest);
        UnitService::new(
            Arc::new(FakeUnits { manifests }),
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

    fn consumer_manifest_with_input(required: Option<bool>) -> Manifest {
        let mut manifest = Manifest::new(vec!["env".to_string()], "bash");
        manifest.inputs.insert(
            "net".to_string(),
            cubtera_domain::InputSpec {
                unit: "network_producer".to_string(),
                dims: None,
                ext: None,
                required,
            },
        );
        manifest
    }

    fn unit_service_with_producer(
        consumer: Manifest,
        producer: Manifest,
        env_meta: serde_json::Value,
        unit_state: Option<Arc<dyn crate::ports::UnitStateRepository>>,
    ) -> UnitService {
        let mut manifests = HashMap::new();
        manifests.insert("network".to_string(), consumer);
        manifests.insert("network_producer".to_string(), producer);
        let service = UnitService::new(
            Arc::new(FakeUnits { manifests }),
            dimension_service(env_meta),
        );
        match unit_state {
            Some(store) => service.with_unit_state(store),
            None => service,
        }
    }

    #[tokio::test]
    async fn resolve_inputs_projects_producer_dims_from_consumer_chain() {
        let producer_manifest = Manifest::new(vec!["env".to_string()], "tf");
        let record = cubtera_domain::UnitStateRecord {
            org: "cubtera".to_string(),
            unit: "network_producer".to_string(),
            dims: vec!["env:prod".to_string()],
            ext: vec![],
            outputs: json!({"vpc_id": "vpc-1"}),
            updated_at: 0,
        };
        let store: Arc<dyn crate::ports::UnitStateRepository> =
            Arc::new(FakeUnitState::with_record(record));
        let service = unit_service_with_producer(
            consumer_manifest_with_input(None),
            producer_manifest,
            json!({}),
            Some(store),
        );

        let unit = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();

        assert_eq!(
            unit.resolved_inputs.get("net"),
            Some(&json!({"vpc_id": "vpc-1"}))
        );
    }

    #[tokio::test]
    async fn resolve_inputs_fails_when_required_input_missing() {
        let producer_manifest = Manifest::new(vec!["env".to_string()], "tf");
        let store: Arc<dyn crate::ports::UnitStateRepository> = Arc::new(FakeUnitState::default());
        let service = unit_service_with_producer(
            consumer_manifest_with_input(None),
            producer_manifest,
            json!({}),
            Some(store),
        );

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::NotFound { .. })));
    }

    #[tokio::test]
    async fn resolve_inputs_skips_when_optional_input_missing() {
        let producer_manifest = Manifest::new(vec!["env".to_string()], "tf");
        let store: Arc<dyn crate::ports::UnitStateRepository> = Arc::new(FakeUnitState::default());
        let service = unit_service_with_producer(
            consumer_manifest_with_input(Some(false)),
            producer_manifest,
            json!({}),
            Some(store),
        );

        let unit = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();
        assert!(!unit.resolved_inputs.contains_key("net"));
    }

    #[tokio::test]
    async fn resolve_inputs_fails_when_no_store_configured() {
        let producer_manifest = Manifest::new(vec!["env".to_string()], "tf");
        let service = unit_service_with_producer(
            consumer_manifest_with_input(None),
            producer_manifest,
            json!({}),
            None,
        );

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::Config(_))));
    }

    #[tokio::test]
    async fn resolve_inputs_fails_when_producer_requires_unresolvable_dimension() {
        // Producer requires "dc", which the consumer never resolved (only "env").
        let producer_manifest = Manifest::new(vec!["dc".to_string()], "tf");
        let store: Arc<dyn crate::ports::UnitStateRepository> = Arc::new(FakeUnitState::default());
        let service = unit_service_with_producer(
            consumer_manifest_with_input(None),
            producer_manifest,
            json!({}),
            Some(store),
        );

        let result = service
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::Domain(_))));
    }
}
