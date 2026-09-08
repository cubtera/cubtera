//! `Validate`: check dimensions (and the inventory's own dim-type graph)
//! for problems v2 either didn't check at all or only checked one
//! dimension at a time (`cubtera im validate <type> <name>`, schema-only).
//!
//! Two things get checked per dimension, closing v2's H5
//! (`AGENTS.md`: "missing/cyclic parents were silently accepted"):
//! - its "meta" section against its type's JSON schema (same rule v2 already
//!   has, `DimensionService::validate_schema`);
//! - its resolved parent's *type* against the [`DimGraph`]'s declared
//!   gap-fill edge for this type, if one exists - v2 never checks this at
//!   all, so a `dc` whose `meta.parent` accidentally points at another `dc`
//!   instead of an `env` resolves "successfully" and silently produces a
//!   wrong `key_path`.

use crate::error::AppResult;
use crate::resolve::ResolveUseCase;
use cubtera_kernel::{DimRef, Ident};
use cubtera_model::DimGraph;
use serde::Serialize;
use std::fmt;

/// Outcome of validating one dimension.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DimensionValidation {
    pub key: DimRef,
    /// Violations of the type's JSON schema (empty if the type is
    /// `SchemaSpec::Permissive` or the "meta" section satisfies it).
    pub schema_errors: Vec<String>,
    /// Mismatches between the resolved `meta.parent` type and the type's
    /// declared gap-fill edge target, if any.
    pub graph_errors: Vec<String>,
}

impl DimensionValidation {
    pub fn is_ok(&self) -> bool {
        self.schema_errors.is_empty() && self.graph_errors.is_empty()
    }
}

impl fmt::Display for DimensionValidation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_ok() {
            return write!(f, "{}: valid", self.key);
        }
        write!(f, "{}: invalid", self.key)?;
        for e in self.schema_errors.iter().chain(self.graph_errors.iter()) {
            write!(f, "\n  - {e}")?;
        }
        Ok(())
    }
}

/// Outcome of validating every dimension of every type in a chain
/// (`cubtera validate`'s "static check of the fleet").
#[derive(Debug, Clone, Default, Serialize)]
pub struct FleetValidation {
    pub results: Vec<DimensionValidation>,
}

impl FleetValidation {
    pub fn is_ok(&self) -> bool {
        self.results.iter().all(DimensionValidation::is_ok)
    }

    pub fn failures(&self) -> impl Iterator<Item = &DimensionValidation> {
        self.results.iter().filter(|r| !r.is_ok())
    }
}

pub struct ValidateUseCase {
    resolve: ResolveUseCase,
}

impl ValidateUseCase {
    pub fn new(resolve: ResolveUseCase) -> Self {
        Self { resolve }
    }

    /// Validate one dimension against `graph`.
    pub async fn validate_dimension(
        &self,
        graph: &DimGraph,
        org: &str,
        dim_type: &Ident,
        name: &Ident,
    ) -> AppResult<DimensionValidation> {
        let dim = self.resolve.resolve(org, dim_type, name).await?;

        let schema_errors = graph
            .get(dim_type)
            .map(|def| def.schema.validate(dim.meta()))
            .unwrap_or_default();

        let mut graph_errors = Vec::new();
        if let (Some(def), Some(parent_ref)) = (graph.get(dim_type), &dim.parent_ref) {
            if let Some(edge) = def.gap_fill_edge() {
                if edge.target_type != parent_ref.dim_type {
                    graph_errors.push(format!(
                        "meta.parent {parent_ref} has type {:?}, but {dim_type:?} declares its gap-fill parent as {:?}",
                        parent_ref.dim_type.as_str(),
                        edge.target_type.as_str(),
                    ));
                }
            }
        }

        Ok(DimensionValidation {
            key: dim.key.clone(),
            schema_errors,
            graph_errors,
        })
    }

    /// Validate every dimension of every type in `chain`, in order.
    pub async fn validate_fleet(
        &self,
        graph: &DimGraph,
        org: &str,
        chain: &[Ident],
    ) -> AppResult<FleetValidation> {
        let mut results = Vec::new();
        for dim_type in chain {
            for name in self.resolve.list_names(org, dim_type).await? {
                let name = Ident::parse(&name).map_err(|e| {
                    crate::error::AppError::validation(format!(
                        "inventory returned an unparseable dimension name {name:?}: {e}"
                    ))
                })?;
                results.push(self.validate_dimension(graph, org, dim_type, &name).await?);
            }
        }
        Ok(FleetValidation { results })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{InventoryPort, RawSections};
    use async_trait::async_trait;
    use cubtera_model::{DimEdge, DimTypeDef, SchemaSpec};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

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
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(None)
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
    }

    fn ident(s: &str) -> Ident {
        Ident::parse(s).unwrap()
    }

    fn graph_dome_env_dc() -> DimGraph {
        DimGraph::new()
            .with_type(DimTypeDef::new(ident("dome"), SchemaSpec::Permissive))
            .with_type(
                DimTypeDef::new(ident("env"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                    ident("parent"),
                    ident("dome"),
                    true,
                )),
            )
            .with_type(
                DimTypeDef::new(ident("dc"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                    ident("parent"),
                    ident("env"),
                    true,
                )),
            )
    }

    #[tokio::test]
    async fn well_formed_parent_type_passes() {
        let inv = FakeInventory::new();
        inv.insert("cubtera", "dome", "prod", sections(json!({"meta": {}})));
        inv.insert(
            "cubtera",
            "env",
            "prod",
            sections(json!({"meta": {"parent": "dome:prod"}})),
        );
        let uc = ValidateUseCase::new(ResolveUseCase::new(Arc::new(inv)));
        let result = uc
            .validate_dimension(
                &graph_dome_env_dc(),
                "cubtera",
                &ident("env"),
                &ident("prod"),
            )
            .await
            .unwrap();
        assert!(result.is_ok(), "{result}");
    }

    #[tokio::test]
    async fn parent_of_the_wrong_type_is_a_graph_error() {
        let inv = FakeInventory::new();
        // `dc`'s gap-fill parent should be an `env`, not another `dc` -
        // v2 would resolve this "successfully" and never notice.
        inv.insert("cubtera", "dc", "other", sections(json!({"meta": {}})));
        inv.insert(
            "cubtera",
            "dc",
            "us-east-1",
            sections(json!({"meta": {"parent": "dc:other"}})),
        );
        let uc = ValidateUseCase::new(ResolveUseCase::new(Arc::new(inv)));
        let result = uc
            .validate_dimension(
                &graph_dome_env_dc(),
                "cubtera",
                &ident("dc"),
                &ident("us-east-1"),
            )
            .await
            .unwrap();
        assert!(!result.is_ok());
        assert_eq!(result.graph_errors.len(), 1);
    }

    #[tokio::test]
    async fn schema_violations_are_reported() {
        let inv = FakeInventory::new();
        inv.insert("cubtera", "dc", "us-east-1", sections(json!({"meta": {}})));
        let graph = DimGraph::new().with_type(DimTypeDef::new(
            ident("dc"),
            SchemaSpec::Explicit(json!({
                "type": "object",
                "required": ["region"],
            })),
        ));
        let uc = ValidateUseCase::new(ResolveUseCase::new(Arc::new(inv)));
        let result = uc
            .validate_dimension(&graph, "cubtera", &ident("dc"), &ident("us-east-1"))
            .await
            .unwrap();
        assert!(!result.is_ok());
        assert_eq!(result.schema_errors.len(), 1);
    }

    #[tokio::test]
    async fn validate_fleet_walks_every_type_and_name_in_the_chain() {
        let inv = FakeInventory::new();
        inv.insert("cubtera", "dome", "prod", sections(json!({"meta": {}})));
        inv.insert("cubtera", "dome", "staging", sections(json!({"meta": {}})));
        let uc = ValidateUseCase::new(ResolveUseCase::new(Arc::new(inv)));
        let chain = vec![ident("dome")];
        let report = uc
            .validate_fleet(&graph_dome_env_dc(), "cubtera", &chain)
            .await
            .unwrap();
        assert_eq!(report.results.len(), 2);
        assert!(report.is_ok());
    }
}
