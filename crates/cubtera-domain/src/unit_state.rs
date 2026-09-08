//! Unit state (cross-unit outputs) types and key projection
//!
//! Producers publish a JSON blob of outputs, keyed by the exact dimensions
//! (and optional extensions) they ran with - not their full ancestor chain.
//! Consumers declare `[inputs.<alias>]` in their manifest and either name
//! the producer's dimensions explicitly or let [`project_state_key`] derive
//! them from the consumer's own resolved dimension chain, projected onto
//! the producer's required dimension types (`Manifest::dimensions`).
//!
//! This is deliberately not a DAG: there is no auto-run of the producer,
//! and there is no "bring every state that unit ever published" fallback -
//! an unresolvable projection (a producer dimension type the consumer never
//! resolved, or an ambiguous match) is a hard error, not a guess.

use crate::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

/// Identifies one producer unit's published outputs: the exact `type:name`
/// dimensions (and, if used, extensions) it ran with - the same key a
/// consumer must land on when projecting its own resolved dimensions onto
/// the producer's required dimension types.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct UnitStateKey {
    /// Organization name
    pub org: String,
    /// Producer unit name
    pub unit: String,
    /// `type:name` entries, always kept sorted for a stable canonical form
    pub dims: Vec<String>,
    /// `type:name` extension entries, always kept sorted
    pub ext: Vec<String>,
}

impl UnitStateKey {
    /// Build a key, normalizing `dims`/`ext` into a stable sorted order so
    /// two logically-identical keys always compare/hash equal regardless of
    /// the order their dimensions were resolved in.
    pub fn new(
        org: impl Into<String>,
        unit: impl Into<String>,
        mut dims: Vec<String>,
        mut ext: Vec<String>,
    ) -> Self {
        dims.sort();
        ext.sort();
        Self {
            org: org.into(),
            unit: unit.into(),
            dims,
            ext,
        }
    }

    /// A stable, human-readable string form - used in error messages, CLI
    /// output, and as the FS adapter's on-disk path components.
    pub fn canonical(&self) -> String {
        let mut s = format!("{}/{}@{}", self.org, self.unit, self.dims.join(","));
        if !self.ext.is_empty() {
            s.push('#');
            s.push_str(&self.ext.join(","));
        }
        s
    }

    /// Validating constructor for untrusted input - CLI args (`cubtera
    /// state get/rm`), REST query params (`GET .../units/{name}/state`),
    /// and MCP tool params (`get_unit_state`) all build a `UnitStateKey`
    /// directly from a caller-supplied string with no other validation in
    /// front of it (unlike `cubtera run`, there is no `UnitService`/access
    /// policy in this path). `FsUnitStateRepository::record_path` then
    /// joins `org`/`unit`/every `dims`/`ext` entry straight onto
    /// `unitStatePath` - so an unvalidated `org = "../../etc"` or
    /// `dims = ["../../../tmp/pwned"]` was a direct arbitrary
    /// read/write/delete via three different interfaces at once. This is
    /// the v3 seam fix (docs/specs/2026-09-03-cubtera-v3-architecture.md
    /// ยง4): validate through `cubtera_kernel::{Ident, DimRef}` once, here,
    /// rather than trusting every interface to remember to check.
    ///
    /// [`UnitStateKey::new`] remains available, unvalidated, for internal
    /// callers that already hold trusted data (e.g.
    /// `UnitStateRecord::key()`, built from a `Unit` that was itself
    /// constructed through `UnitService`, which validates independently).
    pub fn try_new(
        org: impl AsRef<str>,
        unit: impl AsRef<str>,
        dims: Vec<String>,
        ext: Vec<String>,
    ) -> DomainResult<Self> {
        let org = validate_ident("org", org.as_ref())?;
        let unit = validate_ident("unit", unit.as_ref())?;
        let dims = dims
            .iter()
            .map(|d| validate_dim_ref("dims", d))
            .collect::<DomainResult<Vec<_>>>()?;
        let ext = ext
            .iter()
            .map(|e| validate_dim_ref("ext", e))
            .collect::<DomainResult<Vec<_>>>()?;
        Ok(Self::new(org, unit, dims, ext))
    }
}

fn validate_ident(field: &'static str, raw: &str) -> DomainResult<String> {
    cubtera_kernel::Ident::parse(raw)
        .map(cubtera_kernel::Ident::into_string)
        .map_err(|e| DomainError::InvalidManifest {
            reason: format!("invalid {field} {raw:?}: {e}"),
        })
}

fn validate_dim_ref(field: &'static str, raw: &str) -> DomainResult<String> {
    cubtera_kernel::DimRef::parse(raw)
        .map(|r| r.key())
        .map_err(|e| DomainError::InvalidManifest {
            reason: format!("invalid {field} entry {raw:?}: {e}"),
        })
}

/// A producer's published outputs, as stored by a `UnitStateRepository`
/// (`cubtera-core::ports::UnitStateRepository`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnitStateRecord {
    /// Organization name
    pub org: String,
    /// Producer unit name
    pub unit: String,
    /// `type:name` entries the producer ran with (its manifest's required
    /// `dimensions`, resolved)
    pub dims: Vec<String>,
    /// `type:name` extension entries the producer ran with, if any
    pub ext: Vec<String>,
    /// The producer's outputs, already normalized (e.g. terraform's
    /// `{name: {value, type, sensitive}}` flattened to `{name: value}` via
    /// [`flatten_tf_outputs`])
    pub outputs: Value,
    /// Unix epoch seconds when this record was published
    pub updated_at: i64,
}

impl UnitStateRecord {
    /// The key this record is stored/looked up under.
    pub fn key(&self) -> UnitStateKey {
        UnitStateKey::new(
            self.org.clone(),
            self.unit.clone(),
            self.dims.clone(),
            self.ext.clone(),
        )
    }
}

/// Project a consumer's resolved dimension chain onto a producer's required
/// dimension types, to find the exact key the producer would have published
/// under. `consumer_key_path` is every `type:name` entry in the consumer's
/// resolved chain (its own provided dimensions plus every ancestor);
/// `producer_dims` is the producer's `Manifest::dimensions` (required
/// types, in order).
///
/// This only ever looks *up* the consumer's own ancestor chain - it cannot
/// invent a dimension the consumer never resolved. A producer that needs a
/// `dc` while the consumer only resolved a `dome` fails here, by design.
pub fn project_state_key(
    consumer_key_path: &[String],
    producer_dims: &[String],
) -> DomainResult<Vec<String>> {
    let mut projected = Vec::with_capacity(producer_dims.len());
    for dim_type in producer_dims {
        let prefix = format!("{dim_type}:");
        let matches: Vec<&String> = consumer_key_path
            .iter()
            .filter(|k| k.starts_with(&prefix))
            .collect();

        let unique: HashSet<&String> = matches.iter().copied().collect();
        match unique.len() {
            0 => {
                return Err(DomainError::InvalidManifest {
                    reason: format!(
                        "cannot resolve unit state: producer requires dimension type \
                         '{dim_type}', which is not present in the consumer's resolved \
                         dimension chain {consumer_key_path:?}"
                    ),
                });
            }
            1 => projected.push((*matches[0]).clone()),
            _ => {
                return Err(DomainError::InvalidManifest {
                    reason: format!(
                        "cannot resolve unit state: ambiguous dimension type '{dim_type}' in \
                         consumer's resolved chain: {matches:?}"
                    ),
                });
            }
        }
    }
    Ok(projected)
}

/// Flatten terraform/opentofu's `output -json` shape
/// (`{"name": {"value": ..., "type": ..., "sensitive": bool}}`) down to
/// `{"name": value}` - the shape every other runner's `cubtera_outputs.json`
/// is expected to already be in.
pub fn flatten_tf_outputs(raw: &Value) -> Value {
    let Some(obj) = raw.as_object() else {
        return raw.clone();
    };
    let mut flat = serde_json::Map::new();
    for (name, entry) in obj {
        let value = entry
            .as_object()
            .and_then(|e| e.get("value"))
            .cloned()
            .unwrap_or_else(|| entry.clone());
        flat.insert(name.clone(), value);
    }
    Value::Object(flat)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn try_new_accepts_valid_input() {
        let key = UnitStateKey::try_new("cubtera", "network", s(&["dome:prod"]), vec![]).unwrap();
        assert_eq!(key.canonical(), "cubtera/network@dome:prod");
    }

    #[test]
    fn try_new_rejects_path_traversal_in_org() {
        assert!(UnitStateKey::try_new("../../etc", "network", vec![], vec![]).is_err());
    }

    #[test]
    fn try_new_rejects_path_traversal_in_unit() {
        assert!(UnitStateKey::try_new("cubtera", "../../etc", vec![], vec![]).is_err());
    }

    #[test]
    fn try_new_rejects_path_traversal_in_dims() {
        assert!(
            UnitStateKey::try_new("cubtera", "network", s(&["../../../tmp/pwned:x"]), vec![])
                .is_err()
        );
    }

    #[test]
    fn try_new_rejects_path_traversal_in_ext() {
        assert!(
            UnitStateKey::try_new("cubtera", "network", vec![], s(&["../../../tmp/pwned:x"]))
                .is_err()
        );
    }

    #[test]
    fn try_new_rejects_dim_without_colon() {
        assert!(UnitStateKey::try_new("cubtera", "network", s(&["no-colon"]), vec![]).is_err());
    }

    #[test]
    fn canonical_is_stable_regardless_of_input_order() {
        let a = UnitStateKey::new("cubtera", "network", s(&["env:prod", "dome:prod"]), vec![]);
        let b = UnitStateKey::new("cubtera", "network", s(&["dome:prod", "env:prod"]), vec![]);
        assert_eq!(a.canonical(), b.canonical());
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_includes_extensions() {
        let key = UnitStateKey::new("cubtera", "network", s(&["dome:prod"]), s(&["index:0"]));
        assert_eq!(key.canonical(), "cubtera/network@dome:prod#index:0");
    }

    #[test]
    fn project_ancestor_type_from_deeper_consumer_chain() {
        let chain = s(&["dome:prod", "env:prod", "dc:stg1"]);
        let projected = project_state_key(&chain, &s(&["dome"])).unwrap();
        assert_eq!(projected, s(&["dome:prod"]));
    }

    #[test]
    fn project_multiple_producer_dims_preserves_order() {
        let chain = s(&["dome:prod", "env:prod", "dc:stg1"]);
        let projected = project_state_key(&chain, &s(&["dome", "env"])).unwrap();
        assert_eq!(projected, s(&["dome:prod", "env:prod"]));
    }

    #[test]
    fn project_fails_when_producer_dim_type_missing_from_chain() {
        let chain = s(&["dome:prod"]);
        let err = project_state_key(&chain, &s(&["dc"])).unwrap_err();
        assert!(matches!(err, DomainError::InvalidManifest { .. }));
    }

    #[test]
    fn project_fails_on_ambiguous_dimension_type() {
        // Two different `dc` entries in the same chain (shouldn't normally
        // happen, but the algorithm must not silently pick one).
        let chain = s(&["dc:use1", "dc:use2"]);
        let err = project_state_key(&chain, &s(&["dc"])).unwrap_err();
        assert!(matches!(err, DomainError::InvalidManifest { .. }));
    }

    #[test]
    fn flatten_tf_outputs_extracts_value_field() {
        let raw = serde_json::json!({
            "vpc_id": {"value": "vpc-123", "type": "string", "sensitive": false},
            "count": {"value": 3, "type": "number", "sensitive": false}
        });
        let flat = flatten_tf_outputs(&raw);
        assert_eq!(flat["vpc_id"], "vpc-123");
        assert_eq!(flat["count"], 3);
    }

    #[test]
    fn flatten_tf_outputs_passes_through_non_object() {
        let raw = serde_json::json!("not-an-object");
        assert_eq!(flatten_tf_outputs(&raw), raw);
    }
}
