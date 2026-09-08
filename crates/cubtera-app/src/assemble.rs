//! `AssembleUseCase`: build a [`cubtera_model::Unit`] from `InventoryPort`/
//! `UnitPort` - the v3-native equivalent of v2's
//! `cubtera_core::services::UnitService::build_unit_with_extensions`.
//!
//! Deliberately does **not** resolve `[inputs.<alias>]` - that's already
//! native in v3, handled by [`crate::run::RunUseCase`] against
//! `cubtera-store`'s `OutputSet`s (schema-checked, revision-tracked),
//! *not* against a legacy `UnitStateRepository`. A caller building a `Unit`
//! for `plan`/`apply` gets `Unit.resolved_inputs` empty by construction;
//! `RunUseCase` fills in its own `variables` separately. Callers that only
//! need the materialized workspace (dimension data, includes, access
//! policy) - `plan`/`apply`/`explain --dry-run` - never notice the
//! difference.

use crate::error::{AppError, AppResult};
use crate::ports::{InventoryPort, UnitPort};
use crate::resolve::ResolveUseCase;
use cubtera_kernel::{DimRef, Ident};
use cubtera_model::{AccessDecision, AccessPolicy, DimensionAccessContext, IncludeEntry, Unit};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub struct AssembleUseCase {
    inventory: Arc<dyn InventoryPort>,
    units: Arc<dyn UnitPort>,
    resolve: ResolveUseCase,
}

impl AssembleUseCase {
    pub fn new(inventory: Arc<dyn InventoryPort>, units: Arc<dyn UnitPort>) -> Self {
        let resolve = ResolveUseCase::new(inventory.clone());
        Self {
            inventory,
            units,
            resolve,
        }
    }

    /// Build a unit with resolved dimensions (no extensions) - see
    /// [`Self::build_unit_with_extensions`].
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
    /// e.g. `-e index:0` for a sharded deployment - see
    /// `cubtera_model::Unit::with_extensions`).
    ///
    /// Validates `org`/`unit_name`/`extensions` through `cubtera_kernel`
    /// up front (the same kernel-seam rationale as v2's ported version:
    /// every entry point funnels through here, so a crafted `-e
    /// '../../../etc'`/`-u '../../etc'` is a validation error, not a write
    /// outside `tempFolderPath` once `Unit::calculate_temp_folder` joins
    /// these onto a path).
    pub async fn build_unit_with_extensions(
        &self,
        org: &str,
        unit_name: &str,
        dimension_keys: &[String],
        extensions: &[String],
    ) -> AppResult<Unit> {
        Ident::parse(org).map_err(|e| AppError::validation(format!("invalid org {org:?}: {e}")))?;
        Ident::parse(unit_name)
            .map_err(|e| AppError::validation(format!("invalid unit name {unit_name:?}: {e}")))?;
        for ext in extensions {
            DimRef::parse(ext)
                .map_err(|e| AppError::validation(format!("invalid extension {ext:?}: {e}")))?;
        }

        let manifest = self
            .units
            .find_manifest(org, unit_name)
            .await?
            .ok_or_else(|| AppError::not_found("unit", unit_name))?;

        let mut dims: Vec<DimRef> = Vec::new();
        let mut dim_data: HashMap<String, Value> = HashMap::new();
        // Full "type:name" chain of every resolved dimension and its
        // ancestors, so allowList/denyList can gate on a parent dimension.
        let mut dims_tree: HashSet<String> = HashSet::new();
        let mut access_contexts: Vec<DimensionAccessContext> = Vec::new();
        let mut includes: Vec<IncludeEntry> = Vec::new();

        for key in dimension_keys {
            let dim_ref = DimRef::parse(key)
                .map_err(|e| AppError::validation(format!("invalid dimension {key:?}: {e}")))?;

            // Fully assembled dimension data (defaults + parent chain already applied).
            let dimension = self
                .resolve
                .resolve(org, &dim_ref.dim_type, &dim_ref.name)
                .await?;
            dim_data.insert(
                dim_ref.dim_type.as_str().to_string(),
                Value::Object(dimension.sections.clone().into_iter().collect()),
            );

            // v1/v2 parity: default includes are collected before the
            // dimension's own, so a same-named entry from the dimension
            // itself wins once `Unit::materialize`'s steps are applied in
            // order (see `cubtera_exec::apply_materialization_plan`'s
            // "later copy wins" behavior).
            includes.extend(
                self.inventory
                    .list_default_includes(org, dim_ref.dim_type.as_str())
                    .await?,
            );
            includes.extend(
                self.inventory
                    .list_includes(org, dim_ref.dim_type.as_str(), dim_ref.name.as_str())
                    .await?,
            );

            dims_tree.extend(dimension.key_path.iter().map(DimRef::key));
            let affinity_tags = dimension
                .meta()
                .get("affinity_tags")
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            access_contexts.push(DimensionAccessContext {
                key: dim_ref.key(),
                affinity_tags,
            });

            dims.push(dim_ref);
        }

        let mut unit = Unit::new(unit_name, org, manifest);
        for dim_ref in dims {
            unit = unit.with_dimension(dim_ref);
        }
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

        if let Some(path) = self.units.get_unit_path(org, unit_name).await? {
            unit = unit.with_unit_path(path);
        }

        if !unit.has_all_required_dimensions() {
            let missing = unit.missing_dimensions();
            return Err(AppError::validation(format!(
                "missing required dimensions: {}",
                missing.join(", ")
            )));
        }

        match AccessPolicy::evaluate(&unit.manifest, &dims_tree, &access_contexts) {
            AccessDecision::Allowed => {}
            AccessDecision::Denied { reason } => {
                return Err(AppError::access_denied(format!(
                    "unit '{unit_name}' denied for dimensions {dims_tree:?}: {reason}"
                )));
            }
        }

        Ok(unit)
    }

    /// List every known unit name for `org` - delegates to `UnitPort`.
    pub async fn list_units(&self, org: &str) -> AppResult<Vec<String>> {
        self.units.list_units(org).await
    }

    /// Fetch `unit_name`'s manifest without resolving any dimensions.
    pub async fn get_manifest(
        &self,
        org: &str,
        unit_name: &str,
    ) -> AppResult<cubtera_model::Manifest> {
        self.units
            .find_manifest(org, unit_name)
            .await?
            .ok_or_else(|| AppError::not_found("unit", unit_name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RawSections;
    use async_trait::async_trait;
    use cubtera_model::Manifest;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    // org -> dim_type -> name -> sections
    type Data = BTreeMap<String, BTreeMap<String, BTreeMap<String, RawSections>>>;
    // org -> dim_type -> name -> includes
    type IncludesData = BTreeMap<String, BTreeMap<String, BTreeMap<String, Vec<IncludeEntry>>>>;

    #[derive(Default)]
    struct FakeInventory {
        data: Mutex<Data>,
        includes: Mutex<IncludesData>,
    }

    impl FakeInventory {
        fn insert(&self, org: &str, dim_type: &str, name: &str, sections: RawSections) {
            self.data
                .lock()
                .unwrap()
                .entry(org.to_string())
                .or_default()
                .entry(dim_type.to_string())
                .or_default()
                .insert(name.to_string(), sections);
        }

        fn insert_includes(
            &self,
            org: &str,
            dim_type: &str,
            name: &str,
            entries: Vec<IncludeEntry>,
        ) {
            self.includes
                .lock()
                .unwrap()
                .entry(org.to_string())
                .or_default()
                .entry(dim_type.to_string())
                .or_default()
                .insert(name.to_string(), entries);
        }
    }

    fn sections(v: serde_json::Value) -> RawSections {
        match v {
            serde_json::Value::Object(m) => m.into_iter().collect(),
            _ => panic!("expected object"),
        }
    }

    #[async_trait]
    impl InventoryPort for FakeInventory {
        async fn get_raw(
            &self,
            org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(name))
                .cloned())
        }

        async fn get_raw_defaults(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(None)
        }

        async fn get_raw_schema(&self, _org: &str, _dim_type: &str) -> AppResult<Option<Value>> {
            Ok(None)
        }

        async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .map(|n| n.keys().cloned().collect())
                .unwrap_or_default())
        }

        async fn list_includes(
            &self,
            org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Vec<IncludeEntry>> {
            Ok(self
                .includes
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(name))
                .cloned()
                .unwrap_or_default())
        }

        async fn list_default_includes(
            &self,
            org: &str,
            dim_type: &str,
        ) -> AppResult<Vec<IncludeEntry>> {
            Ok(self
                .includes
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(".default"))
                .cloned()
                .unwrap_or_default())
        }
    }

    struct FakeUnits {
        manifests: HashMap<String, Manifest>,
    }

    #[async_trait]
    impl UnitPort for FakeUnits {
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

    fn use_case(manifest: Manifest, env_meta: serde_json::Value) -> AssembleUseCase {
        let inventory = FakeInventory::default();
        inventory.insert(
            "cubtera",
            "env",
            "prod",
            sections(json!({ "meta": env_meta })),
        );
        let mut manifests = HashMap::new();
        manifests.insert("network".to_string(), manifest);
        AssembleUseCase::new(Arc::new(inventory), Arc::new(FakeUnits { manifests }))
    }

    #[tokio::test]
    async fn build_unit_succeeds_with_no_policy_restrictions() {
        let manifest = Manifest::new(vec!["env".to_string()], "tf");
        let uc = use_case(manifest, json!({}));

        let unit = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();
        assert_eq!(unit.name, "network");
        assert_eq!(unit.dimensions[0].key(), "env:prod");
        assert_eq!(unit.unit_path, std::path::PathBuf::from("/units/network"));
    }

    #[tokio::test]
    async fn build_unit_denied_when_dimension_not_in_allow_list() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.allow_list = Some(vec!["env:staging".to_string()]);
        let uc = use_case(manifest, json!({}));

        let result = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::AccessDenied(_))));
    }

    #[tokio::test]
    async fn build_unit_denied_when_dimension_missing_affinity_tag() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.affinity_tags = Some(vec!["critical".to_string()]);
        let uc = use_case(manifest, json!({"affinity_tags": ["core"]}));

        let result = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::AccessDenied(_))));
    }

    #[tokio::test]
    async fn build_unit_allowed_when_affinity_tags_overlap() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        manifest.affinity_tags = Some(vec!["core".to_string()]);
        let uc = use_case(manifest, json!({"affinity_tags": ["core"]}));

        let result = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn build_unit_fails_validation_for_missing_required_dimension() {
        let manifest = Manifest::new(vec!["env".to_string(), "dc".to_string()], "tf");
        let uc = use_case(manifest, json!({}));

        let result = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await;
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[tokio::test]
    async fn build_unit_fails_for_unknown_unit() {
        let inventory = FakeInventory::default();
        let uc = AssembleUseCase::new(
            Arc::new(inventory),
            Arc::new(FakeUnits {
                manifests: HashMap::new(),
            }),
        );
        let result = uc.build_unit("cubtera", "missing", &[]).await;
        assert!(matches!(result, Err(AppError::NotFound { .. })));
    }

    #[tokio::test]
    async fn build_unit_rejects_a_traversal_extension() {
        let manifest = Manifest::new(vec!["env".to_string()], "tf");
        let uc = use_case(manifest, json!({}));

        let result = uc
            .build_unit_with_extensions(
                "cubtera",
                "network",
                &["env:prod".to_string()],
                &["../../etc:passwd".to_string()],
            )
            .await;
        assert!(matches!(result, Err(AppError::Validation(_))));
    }

    #[tokio::test]
    async fn build_unit_aggregates_dimension_data_into_flat_dim_vars() {
        let manifest = Manifest::new(vec!["env".to_string()], "tf");
        let uc = use_case(manifest, json!({"region": "us-east-1"}));

        let unit = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();
        assert_eq!(unit.dimension_data["env"]["meta"]["region"], "us-east-1");
    }

    #[tokio::test]
    async fn build_unit_merges_default_then_own_includes_in_order() {
        let manifest = Manifest::new(vec!["env".to_string()], "tf");
        let inventory = FakeInventory::default();
        inventory.insert("cubtera", "env", "prod", sections(json!({"meta": {}})));
        inventory.insert_includes(
            "cubtera",
            "env",
            ".default",
            vec![IncludeEntry {
                name: "notice.txt".to_string(),
                source: "/inv/env/.default:notice.txt".into(),
                is_dir: false,
            }],
        );
        inventory.insert_includes(
            "cubtera",
            "env",
            "prod",
            vec![IncludeEntry {
                name: "notice.txt".to_string(),
                source: "/inv/env/prod:notice.txt".into(),
                is_dir: false,
            }],
        );
        let mut manifests = HashMap::new();
        manifests.insert("network".to_string(), manifest);
        let uc = AssembleUseCase::new(Arc::new(inventory), Arc::new(FakeUnits { manifests }));

        let unit = uc
            .build_unit("cubtera", "network", &["env:prod".to_string()])
            .await
            .unwrap();

        // Both entries are kept (so a later materialization step wins),
        // but the dimension's own copy must come *after* the default one.
        assert_eq!(unit.includes.len(), 2);
        assert!(unit.includes[0].source.ends_with(".default:notice.txt"));
        assert!(unit.includes[1].source.ends_with("prod:notice.txt"));
    }
}
