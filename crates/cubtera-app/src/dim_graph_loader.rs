//! Builds a [`DimGraph`] from an ordered dimension-type chain (the same
//! source of truth as v2's `dimRelations`/`DimHierarchy` config) plus
//! whatever `.schema:meta.json` records the inventory actually declares.
//!
//! This is deliberately a *bridge*, not the end state: v3's inventory
//! format eventually declares the graph's edges itself (named, typed,
//! possibly non-linear - section 5.1), instead of inferring one "parent" edge
//! per type from a flat ordered list. Building it this way for P3 means
//! every existing v2 inventory gets real graph validation (unknown
//! target types, gap-fill cycles - both silently accepted in v2, see
//! `AGENTS.md`'s H5) with zero format changes; loosening the "one fixed
//! chain" assumption is P5/P6 work once `Binding`/edges beyond `parent`
//! actually exist.

use crate::error::AppResult;
use crate::ports::InventoryPort;
use cubtera_kernel::Ident;
use cubtera_model::{DimEdge, DimGraph, DimTypeDef, SchemaSpec};

/// Load a [`DimGraph`] for `org`, one [`DimTypeDef`] per entry in `chain`
/// (root first, e.g. `["dome", "env", "dc"]`), wiring a gap-fill `parent`
/// edge from each type to the one before it in the chain. A type without
/// a declared `.schema` record gets [`SchemaSpec::Permissive`] rather than
/// being rejected - schemas are conceptually mandatory in v3 (section 5.1) but
/// migrating every existing inventory to declare one is out of scope for
/// this phase.
pub async fn load_dim_graph(
    inventory: &dyn InventoryPort,
    org: &str,
    chain: &[Ident],
) -> AppResult<DimGraph> {
    let mut graph = DimGraph::new();
    let parent_edge_name = Ident::parse("parent").expect("\"parent\" is a valid Ident");

    for (i, dim_type) in chain.iter().enumerate() {
        let schema = match inventory.get_raw_schema(org, dim_type.as_str()).await? {
            Some(schema) => SchemaSpec::Explicit(schema),
            None => SchemaSpec::Permissive,
        };
        let mut def = DimTypeDef::new(dim_type.clone(), schema);
        if i > 0 {
            def = def.with_edge(DimEdge::new(
                parent_edge_name.clone(),
                chain[i - 1].clone(),
                true,
            ));
        }
        graph = graph.with_type(def);
    }

    Ok(graph)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::AppResult;
    use async_trait::async_trait;
    use serde_json::{json, Value};
    use std::collections::BTreeMap;

    struct FakeInventory {
        schemas: BTreeMap<&'static str, Value>,
    }

    #[async_trait]
    impl InventoryPort for FakeInventory {
        async fn get_raw(
            &self,
            _org: &str,
            _dim_type: &str,
            _name: &str,
        ) -> AppResult<Option<crate::ports::RawSections>> {
            Ok(None)
        }

        async fn get_raw_defaults(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<crate::ports::RawSections>> {
            Ok(None)
        }

        async fn get_raw_schema(&self, _org: &str, dim_type: &str) -> AppResult<Option<Value>> {
            Ok(self.schemas.get(dim_type).cloned())
        }

        async fn list_names(&self, _org: &str, _dim_type: &str) -> AppResult<Vec<String>> {
            Ok(Vec::new())
        }
    }

    fn ident(s: &str) -> Ident {
        Ident::parse(s).unwrap()
    }

    #[tokio::test]
    async fn chains_gap_fill_parent_edges_in_order() {
        let inventory = FakeInventory {
            schemas: BTreeMap::new(),
        };
        let chain = vec![ident("dome"), ident("env"), ident("dc")];
        let graph = load_dim_graph(&inventory, "cubtera", &chain).await.unwrap();

        assert!(graph.get(&ident("dome")).unwrap().gap_fill_edge().is_none());
        let env_edge = graph.get(&ident("env")).unwrap().gap_fill_edge().unwrap();
        assert_eq!(env_edge.target_type, ident("dome"));
        let dc_edge = graph.get(&ident("dc")).unwrap().gap_fill_edge().unwrap();
        assert_eq!(dc_edge.target_type, ident("env"));

        assert_eq!(graph.validate(), Ok(()));
    }

    #[tokio::test]
    async fn missing_schema_is_permissive_declared_schema_is_explicit() {
        let mut schemas = BTreeMap::new();
        schemas.insert("dc", json!({"type": "object"}));
        let inventory = FakeInventory { schemas };
        let chain = vec![ident("env"), ident("dc")];
        let graph = load_dim_graph(&inventory, "cubtera", &chain).await.unwrap();

        assert!(graph.get(&ident("env")).unwrap().schema.is_permissive());
        assert!(!graph.get(&ident("dc")).unwrap().schema.is_permissive());
    }
}
