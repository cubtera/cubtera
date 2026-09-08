//! Resolved dimension: gap-filled sections with field-level provenance,
//! a resolved parent chain, and a content hash - v3's replacement for
//! v2's `cubtera_domain::Dimension`
//! (docs/specs/2026-09-03-cubtera-v3-architecture.md ยง5.2).
//!
//! Unlike v2's `Dimension::assemble` (which is also a pure function, but
//! bundles "read the parent ref, recurse" into the same module as the
//! merge itself), this type only does the parts that need zero I/O: merge
//! `own` sections with the type's `.default` sections (recording
//! provenance via [`crate::gap_fill_merge_with_provenance`]) and, given an
//! *already resolved* `parent`, fold its `key_path` into this dimension's
//! own. Actually walking `meta.parent` to find and fetch that parent is
//! `cubtera-app`'s job (P3's `ResolveUseCase`) - it has the
//! `InventoryPort` this crate is not allowed to depend on.

use crate::provenance::{gap_fill_merge_with_provenance, FieldProvenance};
use cubtera_kernel::{Digest, DimRef};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// A fully assembled dimension: `own` gap-filled against `.default`, with
/// provenance for every field that came from a default rather than the
/// dimension's own record, a resolved ancestor chain, and a content hash
/// over the merged sections (deterministic regardless of section/key
/// iteration order - same "never trust iteration order" rule
/// `InstanceId::digest` and `UnitPackage::compute` already apply).
#[derive(Debug, Clone, PartialEq)]
pub struct Dimension {
    pub key: DimRef,
    pub sections: BTreeMap<String, Value>,
    pub provenance: BTreeMap<String, FieldProvenance>,
    pub parent_ref: Option<DimRef>,
    pub key_path: Vec<DimRef>,
    pub content_hash: Digest,
}

impl Dimension {
    /// Assemble one dimension from its own sections and (optionally) its
    /// type's `.default` sections, gap-filling per v2 semantics (own wins;
    /// missing keys/sections are copied from defaults; nested objects
    /// recurse) and recording provenance for anything that came from
    /// `defaults`. `parent`, if given, must already be fully resolved -
    /// its `key_path` is prefixed onto this dimension's own key.
    pub fn assemble(
        key: DimRef,
        own_sections: BTreeMap<String, Value>,
        defaults_sections: Option<&BTreeMap<String, Value>>,
        parent: Option<&Dimension>,
    ) -> Self {
        let default_source = key.dim_type.clone();
        let mut data = Value::Object(map_from_sections(own_sections));
        let mut provenance = BTreeMap::new();

        if let Some(defaults) = defaults_sections {
            let defaults_value = Value::Object(map_from_sections(defaults.clone()));
            gap_fill_merge_with_provenance(
                &mut data,
                &defaults_value,
                &default_source,
                "",
                &mut provenance,
            );
        }

        let mut sections = sections_from_value(data);
        sections
            .entry("meta".to_string())
            .or_insert_with(|| Value::Object(Map::new()));

        let parent_ref = sections
            .get("meta")
            .and_then(|m| m.get("parent"))
            .and_then(|v| v.as_str())
            .and_then(|s| DimRef::parse(s).ok());

        let mut key_path = match parent {
            Some(p) => p.key_path.clone(),
            None => Vec::new(),
        };
        key_path.push(key.clone());

        let content_hash = content_hash_of(&sections);

        Self {
            key,
            sections,
            provenance,
            parent_ref,
            key_path,
            content_hash,
        }
    }

    /// The "meta" section - always present once assembled, even if empty.
    pub fn meta(&self) -> &Value {
        self.sections.get("meta").unwrap_or(&Value::Null)
    }

    /// A named section (e.g. "manifest"), if present.
    pub fn section(&self, name: &str) -> Option<&Value> {
        self.sections.get(name)
    }

    /// Render this dimension as a full resource: its section data plus
    /// resolution metadata that isn't itself part of any section (`name`,
    /// `type`, `parent`, `key_path`, `content_hash`, `kids`) - the same
    /// shape v2's `cubtera_domain::Dimension::to_response_json` produced,
    /// so REST API/CLI/MCP responses don't change shape across the
    /// rewire. `kids` is a pure function's input rather than a field on
    /// `Dimension` itself (computing it needs `InventoryPort::list_names`,
    /// I/O this crate can't do) - callers resolve it via
    /// `cubtera_app::ResolveUseCase::get_children` first.
    pub fn to_response_json(&self, kids: &[String]) -> Value {
        let mut obj: Map<String, Value> = self
            .sections
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        obj.insert("name".to_string(), Value::String(self.key.name.to_string()));
        obj.insert(
            "type".to_string(),
            Value::String(self.key.dim_type.to_string()),
        );
        obj.insert(
            "parent".to_string(),
            self.parent_ref
                .as_ref()
                .map(|r| Value::String(r.to_string()))
                .unwrap_or(Value::Null),
        );
        obj.insert(
            "key_path".to_string(),
            Value::Array(
                self.key_path
                    .iter()
                    .map(|r| Value::String(r.to_string()))
                    .collect(),
            ),
        );
        obj.insert(
            "content_hash".to_string(),
            Value::String(self.content_hash.to_string()),
        );
        obj.insert(
            "kids".to_string(),
            Value::Array(kids.iter().cloned().map(Value::String).collect()),
        );
        Value::Object(obj)
    }
}

fn map_from_sections(sections: BTreeMap<String, Value>) -> Map<String, Value> {
    sections.into_iter().collect()
}

fn sections_from_value(value: Value) -> BTreeMap<String, Value> {
    match value {
        Value::Object(map) => map.into_iter().collect(),
        _ => BTreeMap::new(),
    }
}

/// Content hash over the merged sections, canonicalized (object keys
/// sorted, recursively) before hashing so the hash never depends on
/// `serde_json`'s insertion-order preservation.
fn content_hash_of(sections: &BTreeMap<String, Value>) -> Digest {
    let value = Value::Object(sections.clone().into_iter().collect());
    let ordered = order_json(&value);
    let bytes = serde_json::to_vec(&ordered).unwrap_or_default();
    Digest::of(&bytes)
}

fn order_json(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let ordered: BTreeMap<_, _> = map
                .iter()
                .map(|(k, v)| (k.clone(), order_json(v)))
                .collect();
            Value::Object(ordered.into_iter().collect())
        }
        Value::Array(arr) => Value::Array(arr.iter().map(order_json).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_kernel::Ident;
    use serde_json::json;

    fn dim_ref(s: &str) -> DimRef {
        DimRef::parse(s).unwrap()
    }

    fn sections(json: Value) -> BTreeMap<String, Value> {
        match json {
            Value::Object(map) => map.into_iter().collect(),
            _ => panic!("expected object"),
        }
    }

    #[test]
    fn assemble_wraps_meta_when_absent() {
        let dim = Dimension::assemble(dim_ref("dc:stg1"), BTreeMap::new(), None, None);
        assert_eq!(dim.meta(), &json!({}));
        assert_eq!(dim.key_path, vec![dim_ref("dc:stg1")]);
    }

    #[test]
    fn assemble_gap_fills_defaults_and_records_provenance() {
        let own = sections(json!({"meta": {"region": "us-east-2"}}));
        let defaults =
            sections(json!({"meta": {"region": "us-east-1", "vpc_cidr": "10.0.0.0/16"}}));
        let dim = Dimension::assemble(dim_ref("dc:stg1"), own, Some(&defaults), None);

        assert_eq!(dim.meta()["region"], "us-east-2"); // own wins
        assert_eq!(dim.meta()["vpc_cidr"], "10.0.0.0/16"); // gap-filled
        assert_eq!(
            crate::field_provenance_for(&dim.provenance, "meta/vpc_cidr").map(|p| &p.source),
            Some(&crate::ProvenanceSource::Default(
                Ident::parse("dc").unwrap()
            ))
        );
        assert_eq!(
            crate::field_provenance_for(&dim.provenance, "meta/region").map(|p| &p.source),
            Some(&crate::ProvenanceSource::Own) // present in both, own wins
        );
    }

    #[test]
    fn assemble_extracts_parent_ref_from_merged_meta() {
        let own = sections(json!({"meta": {"parent": "env:prod"}}));
        let dim = Dimension::assemble(dim_ref("dc:stg1"), own, None, None);
        assert_eq!(dim.parent_ref, Some(dim_ref("env:prod")));
    }

    #[test]
    fn assemble_prefixes_parent_key_path() {
        let root = Dimension::assemble(dim_ref("dome:prod"), BTreeMap::new(), None, None);
        let own = sections(json!({"meta": {"parent": "dome:prod"}}));
        let child = Dimension::assemble(dim_ref("env:prod"), own, None, Some(&root));
        assert_eq!(
            child.key_path,
            vec![dim_ref("dome:prod"), dim_ref("env:prod")]
        );
    }

    #[test]
    fn content_hash_is_independent_of_key_iteration_order() {
        let a = sections(json!({"meta": {"a": 1, "b": 2}}));
        let b = sections(json!({"meta": {"b": 2, "a": 1}}));
        let dim_a = Dimension::assemble(dim_ref("dc:x"), a, None, None);
        let dim_b = Dimension::assemble(dim_ref("dc:x"), b, None, None);
        assert_eq!(dim_a.content_hash, dim_b.content_hash);
    }

    #[test]
    fn to_response_json_embeds_resolution_metadata() {
        let root = Dimension::assemble(dim_ref("dome:prod"), BTreeMap::new(), None, None);
        let own = sections(json!({"meta": {"parent": "dome:prod", "region": "us-east-1"}}));
        let child = Dimension::assemble(dim_ref("env:prod"), own, None, Some(&root));

        let json = child.to_response_json(&["dc:prod-use1".to_string()]);
        assert_eq!(json["name"], "prod");
        assert_eq!(json["type"], "env");
        assert_eq!(json["parent"], "dome:prod");
        assert_eq!(json["key_path"], json!(["dome:prod", "env:prod"]));
        assert_eq!(json["kids"], json!(["dc:prod-use1"]));
        assert_eq!(json["meta"]["region"], "us-east-1");
        assert!(json["content_hash"].is_string());
    }

    #[test]
    fn content_hash_changes_when_data_changes() {
        let a = sections(json!({"meta": {"region": "us-east-1"}}));
        let b = sections(json!({"meta": {"region": "us-east-2"}}));
        let dim_a = Dimension::assemble(dim_ref("dc:x"), a, None, None);
        let dim_b = Dimension::assemble(dim_ref("dc:x"), b, None, None);
        assert_ne!(dim_a.content_hash, dim_b.content_hash);
    }
}
