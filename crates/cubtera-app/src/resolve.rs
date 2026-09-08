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
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<serde_json::Value>> {
            Ok(None)
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
}
