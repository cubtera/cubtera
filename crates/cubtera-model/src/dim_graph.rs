//! Typed inventory graph.
//!
//! v2 had one hardcoded chain, `dimRelations = ["dome", "env", "dc"]`
//! (`cubtera_domain::DimHierarchy`) - every dimension type has exactly one
//! parent slot, and a non-`parent` relationship between two dimension types
//! (e.g. "this `service` is owned by that `service`", "this `dc` peers
//! with that `dc`") had nowhere to live except an ad hoc field inside
//! `meta`, invisible to `DimHierarchy`/access-policy/`kids` computation.
//!
//! [`DimGraph`] replaces the chain with named, typed edges: a [`DimEdge`]
//! is `(name, target_type, gap_fill)` - `gap_fill = true` marks the one
//! edge per type that participates in `.default` merging (the parent-chain
//! behavior v2 already has, ported via [`crate::gap_fill_merge_with_provenance`]),
//! while any other edge is a plain typed reference, validated for
//! existence at graph-build time instead of silently accepted as an
//! arbitrary `meta` string.

use cubtera_kernel::Ident;
use serde_json::Value;
use std::collections::BTreeMap;
use std::fmt;

/// A dimension type's schema. `Explicit` schemas are validated with
/// `jsonschema`, exactly like v2's `.schema:meta.json`. `Permissive` is the
/// explicit "no constraint declared" state - the model layer's own
/// [`DimTypeDef`] always carries a `SchemaSpec` (schemas are conceptually
/// mandatory in v3, see docs/specs/2026-09-03-cubtera-v3-architecture.md
/// ยง5.1), but a loader for an inventory that predates this rule can
/// construct `Permissive` rather than lying about a schema that doesn't
/// exist - the distinction between "no schema was ever declared" and "this
/// schema happens to accept anything" stays visible instead of collapsing
/// into `Option::None` either way.
#[derive(Debug, Clone, PartialEq)]
pub enum SchemaSpec {
    Permissive,
    Explicit(Value),
}

impl SchemaSpec {
    /// Validate `data` against this schema, mirroring v2's
    /// `cubtera_domain::validate_against_schema`. `Permissive` always
    /// passes. Returns a list of human-readable violation messages (empty
    /// = valid), not a single string, so callers (`cubtera validate`) can
    /// report every violation instead of just the first.
    pub fn validate(&self, data: &Value) -> Vec<String> {
        let Self::Explicit(schema) = self else {
            return Vec::new();
        };
        match jsonschema::validator_for(schema) {
            Ok(validator) => validator
                .iter_errors(data)
                .map(|e| format!("{} at {}", e, e.instance_path()))
                .collect(),
            Err(e) => vec![format!("malformed schema: {e}")],
        }
    }

    pub fn is_permissive(&self) -> bool {
        matches!(self, Self::Permissive)
    }
}

/// A named, typed edge from one dimension type to another.
#[derive(Debug, Clone, PartialEq)]
pub struct DimEdge {
    pub name: Ident,
    pub target_type: Ident,
    /// Whether this edge participates in `.default`/gap-fill merging (the
    /// single v2 "parent" relationship). At most one gap-fill edge per
    /// type is meaningful - [`DimGraph::validate`] does not currently
    /// enforce that (a type declaring two is unusual, not unsafe), but
    /// callers that only care about the gap-fill chain should use
    /// [`DimTypeDef::gap_fill_edge`].
    pub gap_fill: bool,
}

impl DimEdge {
    pub fn new(name: Ident, target_type: Ident, gap_fill: bool) -> Self {
        Self {
            name,
            target_type,
            gap_fill,
        }
    }
}

/// One dimension type's definition in the graph: its schema and its
/// outgoing edges.
#[derive(Debug, Clone, PartialEq)]
pub struct DimTypeDef {
    pub name: Ident,
    pub schema: SchemaSpec,
    pub edges: Vec<DimEdge>,
}

impl DimTypeDef {
    pub fn new(name: Ident, schema: SchemaSpec) -> Self {
        Self {
            name,
            schema,
            edges: Vec::new(),
        }
    }

    pub fn with_edge(mut self, edge: DimEdge) -> Self {
        self.edges.push(edge);
        self
    }

    /// The edge that participates in gap-fill (v2's "parent"), if any.
    pub fn gap_fill_edge(&self) -> Option<&DimEdge> {
        self.edges.iter().find(|e| e.gap_fill)
    }
}

/// Errors [`DimGraph::validate`] can report. Every one of these was a
/// *silent* failure mode in v2 (`AGENTS.md`'s "Notes for AI agents" calls
/// out missing/cyclic parents as accepted without complaint) - here they
/// are reportable, structured values instead.
#[derive(Debug, Clone, PartialEq)]
pub enum GraphError {
    UnknownTargetType {
        from: String,
        edge: String,
        target: String,
    },
    GapFillCycle {
        cycle: Vec<String>,
    },
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTargetType { from, edge, target } => write!(
                f,
                "dimension type {from:?} has edge {edge:?} pointing at undeclared type {target:?}"
            ),
            Self::GapFillCycle { cycle } => {
                write!(f, "gap-fill cycle: {}", cycle.join(" -> "))
            }
        }
    }
}

/// The full typed inventory graph for one org: every declared dimension
/// type and its edges.
#[derive(Debug, Clone, Default)]
pub struct DimGraph {
    types: BTreeMap<Ident, DimTypeDef>,
}

impl DimGraph {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_type(mut self, def: DimTypeDef) -> Self {
        self.types.insert(def.name.clone(), def);
        self
    }

    pub fn get(&self, name: &Ident) -> Option<&DimTypeDef> {
        self.types.get(name)
    }

    pub fn types(&self) -> impl Iterator<Item = &DimTypeDef> {
        self.types.values()
    }

    /// Check every edge's `target_type` is a declared type in this graph,
    /// and that following gap-fill edges from any type never cycles back
    /// on itself - both silently tolerated in v2's single-chain model.
    /// Returns every violation found, not just the first.
    pub fn validate(&self) -> Result<(), Vec<GraphError>> {
        let mut errors = Vec::new();

        for def in self.types.values() {
            for edge in &def.edges {
                if !self.types.contains_key(&edge.target_type) {
                    errors.push(GraphError::UnknownTargetType {
                        from: def.name.to_string(),
                        edge: edge.name.to_string(),
                        target: edge.target_type.to_string(),
                    });
                }
            }
        }

        for def in self.types.values() {
            if let Some(cycle) = self.find_gap_fill_cycle_from(&def.name) {
                errors.push(GraphError::GapFillCycle {
                    cycle: cycle.iter().map(Ident::to_string).collect(),
                });
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }

    fn find_gap_fill_cycle_from(&self, start: &Ident) -> Option<Vec<Ident>> {
        let mut path = vec![start.clone()];
        let mut current = start.clone();
        loop {
            let next = self
                .types
                .get(&current)
                .and_then(|def| def.gap_fill_edge())
                .map(|edge| edge.target_type.clone());
            match next {
                Some(target) if target == *start => {
                    path.push(target);
                    return Some(path);
                }
                Some(target) if path.contains(&target) => {
                    // Cycle not involving `start` - already reported when
                    // we started the walk from a member of that cycle.
                    return None;
                }
                Some(target) => {
                    path.push(target.clone());
                    current = target;
                }
                None => return None,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ident(s: &str) -> Ident {
        Ident::parse(s).unwrap()
    }

    #[test]
    fn validates_clean_chain() {
        let graph = DimGraph::new()
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
            );
        assert_eq!(graph.validate(), Ok(()));
    }

    #[test]
    fn rejects_edge_to_undeclared_type() {
        let graph = DimGraph::new().with_type(
            DimTypeDef::new(ident("dc"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                ident("parent"),
                ident("env"),
                true,
            )),
        );
        let errors = graph.validate().unwrap_err();
        assert_eq!(errors.len(), 1);
        assert!(matches!(errors[0], GraphError::UnknownTargetType { .. }));
    }

    #[test]
    fn rejects_gap_fill_cycle() {
        let graph = DimGraph::new()
            .with_type(
                DimTypeDef::new(ident("a"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                    ident("parent"),
                    ident("b"),
                    true,
                )),
            )
            .with_type(
                DimTypeDef::new(ident("b"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                    ident("parent"),
                    ident("a"),
                    true,
                )),
            );
        let errors = graph.validate().unwrap_err();
        assert!(errors
            .iter()
            .any(|e| matches!(e, GraphError::GapFillCycle { .. })));
    }

    #[test]
    fn self_loop_is_a_cycle() {
        let graph = DimGraph::new().with_type(
            DimTypeDef::new(ident("a"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                ident("parent"),
                ident("a"),
                true,
            )),
        );
        assert!(graph.validate().is_err());
    }

    #[test]
    fn non_gap_fill_edges_never_participate_in_cycle_detection() {
        // "peers with" is a real, non-hierarchical edge - two dc's peering
        // with each other must not be flagged as a gap-fill cycle.
        let graph = DimGraph::new().with_type(
            DimTypeDef::new(ident("dc"), SchemaSpec::Permissive).with_edge(DimEdge::new(
                ident("peer"),
                ident("dc"),
                false,
            )),
        );
        assert_eq!(graph.validate(), Ok(()));
    }

    #[test]
    fn explicit_schema_validates_data() {
        let schema = SchemaSpec::Explicit(serde_json::json!({
            "type": "object",
            "required": ["account_id"],
            "properties": { "account_id": { "type": "string" } }
        }));
        assert!(schema
            .validate(&serde_json::json!({"account_id": "123"}))
            .is_empty());
        assert!(!schema.validate(&serde_json::json!({})).is_empty());
    }

    #[test]
    fn permissive_schema_accepts_anything() {
        assert!(SchemaSpec::Permissive
            .validate(&serde_json::json!(null))
            .is_empty());
    }
}
