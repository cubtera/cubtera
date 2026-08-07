//! Dimension service
//!
//! Owns the business logic that turns raw storage records into fully
//! assembled [`Dimension`]s: defaults gap-fill, parent chain resolution and
//! sibling scans for children. Adapters (`InventoryRepository` impls) stay
//! dumb; this is where the naming-convention-independent rules live.

use crate::error::{AppError, AppResult};
use crate::ports::InventoryRepository;
use cubtera_domain::{DimHierarchy, DimType, Dimension};
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Outcome of validating a dimension's "meta" section against its type's
/// `.schema:meta.json`, if any.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaValidation {
    /// The dimension type has no `.schema` record - nothing to check.
    NoSchema,
    /// Validated successfully against the schema.
    Valid,
    /// Failed validation; each entry is a human-readable error.
    Invalid(Vec<String>),
}

impl SchemaValidation {
    /// `true` unless validation actually failed (`NoSchema`/`Valid` both
    /// count as "fine").
    pub fn is_ok(&self) -> bool {
        !matches!(self, SchemaValidation::Invalid(_))
    }
}

/// Service for dimension operations
pub struct DimensionService {
    repository: Arc<dyn InventoryRepository>,
    hierarchy: DimHierarchy,
}

impl DimensionService {
    /// Create a new dimension service
    pub fn new(repository: Arc<dyn InventoryRepository>, hierarchy: DimHierarchy) -> Self {
        Self {
            repository,
            hierarchy,
        }
    }

    /// Resolve a single dimension: raw + defaults gap-fill + recursively
    /// resolved parent chain. Does not populate `kids` (see [`Self::get_by_name`]).
    fn resolve<'a>(
        &'a self,
        org: &'a str,
        dim_type: &'a str,
        name: &'a str,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<Dimension>>> + Send + 'a>> {
        Box::pin(async move {
            let raw = match self.repository.get_raw(org, dim_type, name).await? {
                Some(raw) => raw,
                None => return Ok(None),
            };
            let defaults = self.repository.get_raw_defaults(org, dim_type).await?;

            // Peek the parent ref without resolving the parent yet (key_path
            // doesn't matter for this preliminary pass).
            let preview =
                Dimension::assemble(DimType::new(dim_type), raw.clone(), defaults.as_ref(), None);

            let parent = match &preview.parent_ref {
                Some(parent_key) => {
                    let (parent_type, parent_name) = Dimension::parse_key(parent_key)?;
                    self.resolve(org, parent_type.as_str(), &parent_name)
                        .await?
                }
                None => None,
            };

            let dim = Dimension::assemble(
                DimType::new(dim_type),
                raw,
                defaults.as_ref(),
                parent.as_ref(),
            );
            Ok(Some(dim))
        })
    }

    /// Compute direct child refs ("type:name") of a dimension, based on
    /// `dim_relations` hierarchy and a scan of the child type's dimensions.
    async fn compute_kids(&self, org: &str, dim_type: &str, name: &str) -> AppResult<Vec<String>> {
        let child_type = match self.hierarchy.child_type(&DimType::new(dim_type)) {
            Some(t) => t.clone(),
            None => return Ok(Vec::new()),
        };

        let parent_key = format!("{}:{}", dim_type, name);
        let child_names = self.repository.list_names(org, child_type.as_str()).await?;

        let mut kids = Vec::new();
        for child_name in child_names {
            if let Some(raw) = self
                .repository
                .get_raw(org, child_type.as_str(), &child_name)
                .await?
            {
                let parent_ref = raw
                    .sections
                    .get("meta")
                    .and_then(|m| m.get("parent"))
                    .and_then(|v| v.as_str());
                if parent_ref == Some(parent_key.as_str()) {
                    kids.push(format!("{}:{}", child_type, child_name));
                }
            }
        }
        Ok(kids)
    }

    /// Get all dimension names by type
    pub async fn get_all_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        self.repository.list_names(org, dim_type).await
    }

    /// Get all dimensions by type with data (kids are not populated; use
    /// [`Self::get_by_name`] for a single, fully-detailed dimension)
    pub async fn get_all(&self, org: &str, dim_type: &str) -> AppResult<Vec<Dimension>> {
        let names = self.repository.list_names(org, dim_type).await?;
        let mut dims = Vec::with_capacity(names.len());
        for name in names {
            if let Some(dim) = self.resolve(org, dim_type, &name).await? {
                dims.push(dim);
            }
        }
        Ok(dims)
    }

    /// Get dimension by name, fully assembled (defaults, parent chain, kids)
    pub async fn get_by_name(&self, org: &str, dim_type: &str, name: &str) -> AppResult<Dimension> {
        let dim = self
            .resolve(org, dim_type, name)
            .await?
            .ok_or_else(|| AppError::not_found("dimension", format!("{}:{}", dim_type, name)))?;
        let kids = self.compute_kids(org, dim_type, name).await?;
        Ok(dim.with_kids(kids))
    }

    /// Get default dimension for type
    pub async fn get_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<Dimension>> {
        let defaults = self.repository.get_raw_defaults(org, dim_type).await?;
        Ok(defaults.map(|raw| Dimension::assemble(DimType::new(dim_type), raw, None, None)))
    }

    /// Get children of a dimension
    pub async fn get_children(
        &self,
        org: &str,
        parent_type: &str,
        parent_name: &str,
    ) -> AppResult<Vec<Dimension>> {
        let kids = self.compute_kids(org, parent_type, parent_name).await?;
        let mut children = Vec::with_capacity(kids.len());
        for key in kids {
            let (child_type, child_name) = Dimension::parse_key(&key)?;
            if let Some(dim) = self.resolve(org, child_type.as_str(), &child_name).await? {
                children.push(dim);
            }
        }
        Ok(children)
    }

    /// Get parent of a dimension
    pub async fn get_parent(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<Dimension>> {
        let dim = match self.resolve(org, dim_type, name).await? {
            Some(dim) => dim,
            None => return Ok(None),
        };
        match &dim.parent_ref {
            Some(parent_key) => {
                let (parent_type, parent_name) = Dimension::parse_key(parent_key)?;
                self.resolve(org, parent_type.as_str(), &parent_name).await
            }
            None => Ok(None),
        }
    }

    /// Get all dimension types
    pub async fn get_types(&self, org: &str) -> AppResult<Vec<String>> {
        self.repository.list_types(org).await
    }

    /// Get all organizations
    pub async fn get_orgs(&self) -> AppResult<Vec<String>> {
        self.repository.list_orgs().await
    }

    /// Validate a dimension exists
    pub async fn validate(&self, org: &str, dim_type: &str, name: &str) -> AppResult<bool> {
        Ok(self
            .repository
            .get_raw(org, dim_type, name)
            .await?
            .is_some())
    }

    /// Get the JSON-schema for a dimension type (its `.schema` record's
    /// "meta" section), if one is defined.
    pub async fn get_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<Value>> {
        let raw = self.repository.get_raw_schema(org, dim_type).await?;
        Ok(raw.and_then(|r| r.sections.get("meta").cloned()))
    }

    /// Validate a dimension's "meta" section against its type's JSON-schema
    /// (`.schema:meta.json`), per the spec in the migration plan. Returns
    /// [`SchemaValidation::NoSchema`] when the type has no schema defined -
    /// that's a valid, non-error outcome (schemas are opt-in).
    pub async fn validate_schema(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<SchemaValidation> {
        let dim = self
            .resolve(org, dim_type, name)
            .await?
            .ok_or_else(|| AppError::not_found("dimension", format!("{}:{}", dim_type, name)))?;

        let Some(schema) = self.get_schema(org, dim_type).await? else {
            return Ok(SchemaValidation::NoSchema);
        };

        let meta = dim.meta().cloned().unwrap_or(Value::Null);
        match cubtera_domain::validate_against_schema(&meta, &schema) {
            Ok(()) => Ok(SchemaValidation::Valid),
            Err(errors) => Ok(SchemaValidation::Invalid(errors)),
        }
    }

    /// Access the configured dimension hierarchy (e.g. for CLI/API introspection)
    pub fn hierarchy(&self) -> &DimHierarchy {
        &self.hierarchy
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppResult;
    use async_trait::async_trait;
    use cubtera_domain::RawDimension;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Mutex;

    // org -> dim_type -> name -> raw
    type InventoryData = HashMap<String, HashMap<String, HashMap<String, RawDimension>>>;

    /// In-memory InventoryRepository for service-level unit tests
    struct FakeInventory {
        data: Mutex<InventoryData>,
    }

    impl FakeInventory {
        fn new() -> Self {
            Self {
                data: Mutex::new(HashMap::new()),
            }
        }

        fn insert(&self, org: &str, dim_type: &str, raw: RawDimension) {
            self.data
                .lock()
                .unwrap()
                .entry(org.to_string())
                .or_default()
                .entry(dim_type.to_string())
                .or_default()
                .insert(raw.name.clone(), raw);
        }
    }

    #[async_trait]
    impl InventoryRepository for FakeInventory {
        async fn get_raw(
            &self,
            org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Option<RawDimension>> {
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
        ) -> AppResult<Option<RawDimension>> {
            Ok(None)
        }

        async fn get_raw_schema(
            &self,
            org: &str,
            dim_type: &str,
        ) -> AppResult<Option<RawDimension>> {
            Ok(self
                .data
                .lock()
                .unwrap()
                .get(org)
                .and_then(|t| t.get(dim_type))
                .and_then(|n| n.get(".schema"))
                .cloned())
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

        async fn save_raw(&self, org: &str, dim_type: &str, raw: &RawDimension) -> AppResult<()> {
            self.insert(org, dim_type, raw.clone());
            Ok(())
        }

        async fn delete_raw(&self, org: &str, dim_type: &str, name: &str) -> AppResult<()> {
            self.data
                .lock()
                .unwrap()
                .get_mut(org)
                .and_then(|t| t.get_mut(dim_type))
                .map(|n| n.remove(name));
            Ok(())
        }
    }

    fn service_with_tree() -> DimensionService {
        let inv = FakeInventory::new();
        inv.insert(
            "cubtera",
            "dome",
            RawDimension::new("prod").with_section("meta", json!({"prod": true})),
        );
        inv.insert(
            "cubtera",
            "env",
            RawDimension::new("prod").with_section("meta", json!({"parent": "dome:prod"})),
        );
        inv.insert(
            "cubtera",
            "dc",
            RawDimension::new("us-east-1")
                .with_section("meta", json!({"parent": "env:prod", "region": "us-east-1"})),
        );
        DimensionService::new(Arc::new(inv), DimHierarchy::new(vec!["dome", "env", "dc"]))
    }

    #[tokio::test]
    async fn test_get_by_name_resolves_parent_chain() {
        let service = service_with_tree();
        let dc = service
            .get_by_name("cubtera", "dc", "us-east-1")
            .await
            .unwrap();
        assert_eq!(dc.parent_ref, Some("env:prod".to_string()));
        assert_eq!(
            dc.key_path,
            vec![
                "dome:prod".to_string(),
                "env:prod".to_string(),
                "dc:us-east-1".to_string()
            ]
        );
    }

    #[tokio::test]
    async fn test_get_children_via_hierarchy() {
        let service = service_with_tree();
        let children = service
            .get_children("cubtera", "env", "prod")
            .await
            .unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].key(), "dc:us-east-1");
    }

    #[tokio::test]
    async fn test_get_by_name_populates_kids() {
        let service = service_with_tree();
        let env = service.get_by_name("cubtera", "env", "prod").await.unwrap();
        assert_eq!(env.kids, vec!["dc:us-east-1".to_string()]);
    }

    #[tokio::test]
    async fn test_get_by_name_not_found() {
        let service = service_with_tree();
        let result = service.get_by_name("cubtera", "dc", "missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_get_parent() {
        let service = service_with_tree();
        let parent = service
            .get_parent("cubtera", "dc", "us-east-1")
            .await
            .unwrap();
        assert_eq!(parent.unwrap().key(), "env:prod");
    }
}
