//! `Resolve`: turn a raw inventory record into a fully assembled
//! [`Dimension`] - v2's `DimensionService::get_by_name`/`resolve`, ported
//! onto `cubtera-model`'s provenance-aware gap-fill
//! ([`cubtera_model::Dimension::assemble`]) instead of v2's
//! `cubtera_domain::Dimension::assemble`, which throws the provenance
//! away.

use crate::error::{AppError, AppResult};
use crate::ports::InventoryPort;
use cubtera_kernel::{DimRef, Ident};
use cubtera_model::Dimension;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub struct ResolveUseCase {
    inventory: Arc<dyn InventoryPort>,
}

impl ResolveUseCase {
    pub fn new(inventory: Arc<dyn InventoryPort>) -> Self {
        Self { inventory }
    }

    /// Resolve one dimension by type and name: own record + `.default`
    /// gap-fill + recursively resolved parent chain (following
    /// `meta.parent`). Returns [`AppError::NotFound`] if `name` doesn't
    /// exist for `dim_type`.
    pub async fn resolve(&self, org: &str, dim_type: &Ident, name: &Ident) -> AppResult<Dimension> {
        let key = DimRef::new(dim_type.clone(), name.clone());
        self.resolve_inner(org, key.clone(), Vec::new())
            .await?
            .ok_or_else(|| AppError::not_found("dimension", key.to_string()))
    }

    /// `resolve`, but `None` instead of `NotFound` when the record is
    /// missing - useful for callers that need to distinguish "doesn't
    /// exist" from every other failure (e.g. `cubtera fleet ls`, which
    /// tolerates a name disappearing between `list_names` and `resolve`).
    pub async fn try_resolve(
        &self,
        org: &str,
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<Option<Dimension>> {
        let key = DimRef::new(dim_type.clone(), name.clone());
        self.resolve_inner(org, key, Vec::new()).await
    }

    /// List every dimension name of `dim_type`.
    pub async fn list_names(&self, org: &str, dim_type: &Ident) -> AppResult<Vec<String>> {
        self.inventory.list_names(org, dim_type.as_str()).await
    }

    /// List every dimension type declared for `org` - a directory listing
    /// (`cubtera im get-types`/`GET /v1/{org}/dim-types`).
    pub async fn list_types(&self, org: &str) -> AppResult<Vec<String>> {
        self.inventory.list_types(org).await
    }

    /// List every org the inventory has data for (`cubtera im
    /// get-orgs`/`GET /v1/orgs`).
    pub async fn list_orgs(&self) -> AppResult<Vec<String>> {
        self.inventory.list_orgs().await
    }

    /// `dim_type`'s `.default` record, gap-filled against nothing (it *is*
    /// the defaults) - `None` if the type has no `.default` record at all.
    /// Unlike [`Self::resolve`], this never walks a parent chain: defaults
    /// are per-type, not per-instance, so there is no `meta.parent` to
    /// follow.
    pub async fn get_defaults(&self, org: &str, dim_type: &Ident) -> AppResult<Option<Dimension>> {
        let defaults = self
            .inventory
            .get_raw_defaults(org, dim_type.as_str())
            .await?;
        Ok(defaults.map(|raw| {
            // `.default` is a per-type pseudo-record, not a real
            // `type:name` dimension - `Ident::parse(".default")` would
            // reject the leading dot, so we reuse `dim_type` for both
            // halves of the synthetic key. Nothing reads `key`/`key_path`
            // off the result; callers only care about `sections`/`meta()`.
            let key = DimRef::new(dim_type.clone(), dim_type.clone());
            Dimension::assemble(key, raw, None, None)
        }))
    }

    /// The JSON-schema for `dim_type` (its `.schema` record's "meta"
    /// section), if one is defined - a direct passthrough, schemas have
    /// no gap-fill/parent-chain concept of their own.
    pub async fn get_schema(&self, org: &str, dim_type: &Ident) -> AppResult<Option<Value>> {
        self.inventory.get_raw_schema(org, dim_type.as_str()).await
    }

    /// `name`'s resolved parent, if it has one - `None` both when `name`
    /// doesn't exist and when it exists but has no `meta.parent` (callers
    /// that need to distinguish those should call [`Self::try_resolve`]
    /// first).
    pub async fn get_parent(
        &self,
        org: &str,
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<Option<Dimension>> {
        let dim = match self.try_resolve(org, dim_type, name).await? {
            Some(dim) => dim,
            None => return Ok(None),
        };
        match &dim.parent_ref {
            Some(parent_ref) => {
                self.try_resolve(org, &parent_ref.dim_type, &parent_ref.name)
                    .await
            }
            None => Ok(None),
        }
    }

    /// Every dimension of the type immediately below `dim_type` in
    /// `dim_relations` (`Config::dim_relations`, e.g. `["dome", "env",
    /// "dc"]`) whose resolved `meta.parent` points back at `dim_type:name`.
    /// Empty if `dim_type` is the last type in the chain, or has no
    /// children yet - never an error either way, matching v2's
    /// `DimensionService::get_children`/`compute_kids`.
    pub async fn get_children(
        &self,
        org: &str,
        dim_relations: &[String],
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<Vec<Dimension>> {
        let Some(child_type) = child_type_of(dim_relations, dim_type.as_str()) else {
            return Ok(Vec::new());
        };
        let child_type = Ident::parse(&child_type)?;
        let parent_key = format!("{dim_type}:{name}");

        let child_names = self.inventory.list_names(org, child_type.as_str()).await?;
        let mut children = Vec::new();
        for child_name in child_names {
            let child_name = Ident::parse(&child_name)?;
            if let Some(dim) = self.try_resolve(org, &child_type, &child_name).await? {
                if dim.parent_ref.as_ref().map(DimRef::to_string) == Some(parent_key.clone()) {
                    children.push(dim);
                }
            }
        }
        Ok(children)
    }

    /// `type:name` refs of every direct child of `dim_type:name` - the
    /// same computation as [`Self::get_children`], just names instead of
    /// fully-resolved `Dimension`s (what [`cubtera_model::Dimension::to_response_json`]'s
    /// `kids` field wants, without paying for a full resolve per child).
    pub async fn kids_of(
        &self,
        org: &str,
        dim_relations: &[String],
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<Vec<String>> {
        Ok(self
            .get_children(org, dim_relations, dim_type, name)
            .await?
            .iter()
            .map(|d| d.key.to_string())
            .collect())
    }

    /// Validate `name`'s "meta" section against `dim_type`'s JSON-schema
    /// (`.schema:meta.json`), if one is defined. Every violation is
    /// returned as a human-readable string; an empty vec means either
    /// "valid" or "no schema declared" - both are a non-error outcome,
    /// schemas are opt-in (matches v2's `SchemaValidation::{NoSchema,
    /// Valid}` collapsing to the same `{"valid": true}` response).
    pub async fn validate_schema(
        &self,
        org: &str,
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<Vec<String>> {
        let dim = self.resolve(org, dim_type, name).await?;
        let Some(schema) = self.get_schema(org, dim_type).await? else {
            return Ok(Vec::new());
        };
        Ok(cubtera_model::SchemaSpec::Explicit(schema).validate(dim.meta()))
    }

    fn resolve_inner<'a>(
        &'a self,
        org: &'a str,
        key: DimRef,
        mut visited: Vec<DimRef>,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<Dimension>>> + Send + 'a>> {
        Box::pin(async move {
            if visited.contains(&key) {
                // v2's `DimensionService::resolve` has no cycle guard at
                // all - an operator-authored `meta.parent` cycle across
                // two dimension *instances* (not caught by
                // `DimGraph::validate`, which only checks type-level
                // gap-fill edges) recurses forever. Closing that here is
                // free since this method is being rewritten anyway.
                let cycle = visited
                    .iter()
                    .map(DimRef::to_string)
                    .chain(std::iter::once(key.to_string()))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                return Err(AppError::validation(format!(
                    "cyclic meta.parent chain: {cycle}"
                )));
            }
            visited.push(key.clone());

            let own = self
                .inventory
                .get_raw(org, key.dim_type.as_str(), key.name.as_str())
                .await?;
            let Some(own) = own else {
                return Ok(None);
            };
            let defaults = self
                .inventory
                .get_raw_defaults(org, key.dim_type.as_str())
                .await?;

            // Peek the parent ref without resolving the parent yet -
            // `key_path` doesn't matter for this preliminary pass.
            let preview = Dimension::assemble(key.clone(), own.clone(), defaults.as_ref(), None);

            let parent = match &preview.parent_ref {
                Some(parent_ref) => self.resolve_inner(org, parent_ref.clone(), visited).await?,
                None => None,
            };

            let dim = Dimension::assemble(key, own, defaults.as_ref(), parent.as_ref());
            Ok(Some(dim))
        })
    }
}

/// The dimension type immediately after `dim_type` in `dim_relations`
/// (an ordered parent-to-child chain, e.g. `["dome", "env", "dc"]`) -
/// `None` if `dim_type` isn't in the chain at all, or is its last entry.
/// Ported from v2's `cubtera_domain::DimHierarchy::child_type`.
fn child_type_of(dim_relations: &[String], dim_type: &str) -> Option<String> {
    let pos = dim_relations.iter().position(|t| t == dim_type)?;
    dim_relations.get(pos + 1).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::RawSections;
    use async_trait::async_trait;
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::Mutex;

    // org -> dim_type -> name -> sections
    type Data = BTreeMap<String, BTreeMap<String, BTreeMap<String, RawSections>>>;

    struct FakeInventory {
        data: Mutex<Data>,
    }

    impl FakeInventory {
        fn new() -> Self {
            Self {
                data: Mutex::new(BTreeMap::new()),
            }
        }

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
            org: &str,
            dim_type: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(".default"))
                .cloned())
        }

        async fn get_raw_schema(
            &self,
            org: &str,
            dim_type: &str,
        ) -> AppResult<Option<serde_json::Value>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(".schema"))
                .and_then(|s| s.get("meta"))
                .cloned())
        }

        async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .map(|n| n.keys().filter(|k| !k.starts_with('.')).cloned().collect())
                .unwrap_or_default())
        }

        async fn list_includes(
            &self,
            _org: &str,
            _dim_type: &str,
            _name: &str,
        ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
            Ok(Vec::new())
        }

        async fn list_default_includes(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
            Ok(Vec::new())
        }

        async fn list_types(&self, org: &str) -> AppResult<Vec<String>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .map(|t| t.keys().cloned().collect())
                .unwrap_or_default())
        }

        async fn list_orgs(&self) -> AppResult<Vec<String>> {
            Ok(self.data.lock().unwrap().keys().cloned().collect())
        }
    }

    fn ident(s: &str) -> Ident {
        Ident::parse(s).unwrap()
    }

    fn use_case_with_tree() -> ResolveUseCase {
        let inv = FakeInventory::new();
        inv.insert("cubtera", "dome", "prod", sections(json!({"meta": {}})));
        inv.insert(
            "cubtera",
            "env",
            "prod",
            sections(json!({"meta": {"parent": "dome:prod"}})),
        );
        inv.insert(
            "cubtera",
            "dc",
            "us-east-1",
            sections(json!({"meta": {"parent": "env:prod", "region": "us-east-1"}})),
        );
        ResolveUseCase::new(Arc::new(inv))
    }

    #[tokio::test]
    async fn resolves_full_parent_chain() {
        let uc = use_case_with_tree();
        let dc = uc
            .resolve("cubtera", &ident("dc"), &ident("us-east-1"))
            .await
            .unwrap();
        assert_eq!(
            dc.key_path
                .iter()
                .map(DimRef::to_string)
                .collect::<Vec<_>>(),
            vec!["dome:prod", "env:prod", "dc:us-east-1"]
        );
    }

    #[tokio::test]
    async fn missing_dimension_is_not_found() {
        let uc = use_case_with_tree();
        let err = uc
            .resolve("cubtera", &ident("dc"), &ident("missing"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn try_resolve_returns_none_instead_of_erroring() {
        let uc = use_case_with_tree();
        let result = uc
            .try_resolve("cubtera", &ident("dc"), &ident("missing"))
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn cyclic_parent_chain_errors_instead_of_looping_forever() {
        let inv = FakeInventory::new();
        inv.insert(
            "cubtera",
            "a",
            "x",
            sections(json!({"meta": {"parent": "b:y"}})),
        );
        inv.insert(
            "cubtera",
            "b",
            "y",
            sections(json!({"meta": {"parent": "a:x"}})),
        );
        let uc = ResolveUseCase::new(Arc::new(inv));
        let err = uc
            .resolve("cubtera", &ident("a"), &ident("x"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn list_names_delegates_to_the_port() {
        let uc = use_case_with_tree();
        let names = uc.list_names("cubtera", &ident("dc")).await.unwrap();
        assert_eq!(names, vec!["us-east-1".to_string()]);
    }

    #[tokio::test]
    async fn list_types_and_orgs_delegate_to_the_port() {
        let uc = use_case_with_tree();
        let mut types = uc.list_types("cubtera").await.unwrap();
        types.sort();
        assert_eq!(
            types,
            vec!["dc".to_string(), "dome".to_string(), "env".to_string()]
        );
        assert_eq!(uc.list_orgs().await.unwrap(), vec!["cubtera".to_string()]);
    }

    #[tokio::test]
    async fn get_parent_walks_one_level_up() {
        let uc = use_case_with_tree();
        let parent = uc
            .get_parent("cubtera", &ident("dc"), &ident("us-east-1"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(parent.key.to_string(), "env:prod");

        // The root has no parent.
        assert!(uc
            .get_parent("cubtera", &ident("dome"), &ident("prod"))
            .await
            .unwrap()
            .is_none());

        // A missing dimension has no parent either (not an error).
        assert!(uc
            .get_parent("cubtera", &ident("dc"), &ident("missing"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_children_finds_direct_children_via_dim_relations() {
        let uc = use_case_with_tree();
        let relations = vec!["dome".to_string(), "env".to_string(), "dc".to_string()];

        let children = uc
            .get_children("cubtera", &relations, &ident("env"), &ident("prod"))
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].key.to_string(), "dc:us-east-1");

        // The last type in the chain has no children.
        assert!(uc
            .get_children("cubtera", &relations, &ident("dc"), &ident("us-east-1"))
            .await
            .unwrap()
            .is_empty());

        let kids = uc
            .kids_of("cubtera", &relations, &ident("env"), &ident("prod"))
            .await
            .unwrap();
        assert_eq!(kids, vec!["dc:us-east-1".to_string()]);
    }

    #[tokio::test]
    async fn get_defaults_reads_the_default_record_with_no_gap_fill() {
        let inv = FakeInventory::new();
        inv.insert(
            "cubtera",
            "dc",
            ".default",
            sections(json!({"meta": {"region": "us-east-1"}})),
        );
        let uc = ResolveUseCase::new(Arc::new(inv));

        let defaults = uc
            .get_defaults("cubtera", &ident("dc"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(defaults.meta()["region"], "us-east-1");

        assert!(uc
            .get_defaults("cubtera", &ident("env"))
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn get_schema_and_validate_schema() {
        let inv = FakeInventory::new();
        inv.insert(
            "cubtera",
            "dc",
            "prod",
            sections(json!({"meta": {"name": "x"}})),
        );
        inv.insert(
            "cubtera",
            "dc",
            ".schema",
            sections(json!({"meta": {"type": "object", "required": ["region"]}})),
        );
        let uc = ResolveUseCase::new(Arc::new(inv));

        let schema = uc.get_schema("cubtera", &ident("dc")).await.unwrap();
        assert_eq!(schema.unwrap()["required"][0], "region");

        let errors = uc
            .validate_schema("cubtera", &ident("dc"), &ident("prod"))
            .await
            .unwrap();
        assert_eq!(errors.len(), 1);
        assert!(errors[0].contains("region"));

        // No schema declared for "env" - always valid.
        let inv2 = FakeInventory::new();
        inv2.insert("cubtera", "env", "prod", sections(json!({"meta": {}})));
        let uc2 = ResolveUseCase::new(Arc::new(inv2));
        assert!(uc2
            .validate_schema("cubtera", &ident("env"), &ident("prod"))
            .await
            .unwrap()
            .is_empty());
    }
}
