//! Dimension entity and related types
//!
//! A Dimension represents a logical grouping for infrastructure organization,
//! such as environment, region, or data center.
//!
//! Naming convention compatibility (inventory-on-disk format is a stable
//! interface, see `f1-fs-adapter-naming`):
//! - `{name}.json` / `{name}{sep}meta.json` -> section "meta"
//! - `{name}{sep}{section}.json` -> section "{section}"
//! - `.default{sep}{section}.json` -> defaults for "{section}", gap-filled into data
//! - `.schema{sep}meta.json` -> JSON-schema for the type, exposed as section "schema"
//! - non-json files/dirs -> [`IncludeEntry`] copied into the unit's temp folder
//!
//! Everything above (naming/suffix parsing) is FS-adapter specific and lives in
//! `cubtera-persistence`. This module only assembles a [`Dimension`] from an
//! already-parsed [`RawDimension`]: gap-fill merge with defaults, `meta` wrapping,
//! parent chain (`key_path`) and content hashing (`data_sha`).

use crate::error::{DomainError, DomainResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// Dimension type identifier (e.g., "env", "dc", "dome")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DimType(String);

impl DimType {
    /// Create a new dimension type
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// Get the dimension type as a string slice
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for DimType {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for DimType {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl std::fmt::Display for DimType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A non-JSON file or folder attached to a dimension (copied verbatim into a
/// unit's temp folder at materialization time). Produced by an adapter while
/// parsing the on-disk (or wire) naming convention; the domain treats the
/// contents opaquely.
#[derive(Debug, Clone)]
pub struct IncludeEntry {
    /// Name the entry should have at the destination (suffix stripped)
    pub name: String,
    /// Adapter-specific source location (e.g. absolute path for FS)
    pub source: PathBuf,
    /// Whether this entry is a directory (copied recursively) or a single file
    pub is_dir: bool,
}

/// A raw, adapter-supplied dimension record: sections keyed by their logical
/// name (`meta`, `schema`, or any custom section such as `manifest`), plus
/// attached includes. No business rules (defaults, parent resolution, hashing)
/// have been applied yet - that is the domain's job in [`Dimension::assemble`].
#[derive(Debug, Clone, Default)]
pub struct RawDimension {
    /// Dimension name (e.g., "prod")
    pub name: String,
    /// Raw sections, as parsed from storage (must contain "meta" once assembled)
    pub sections: HashMap<String, Value>,
    /// Non-JSON includes (files/folders) attached to this dimension
    pub includes: Vec<IncludeEntry>,
}

impl RawDimension {
    /// Create an empty raw dimension with the given name
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sections: HashMap::new(),
            includes: Vec::new(),
        }
    }

    /// Add/replace a section
    pub fn with_section(mut self, key: impl Into<String>, value: Value) -> Self {
        self.sections.insert(key.into(), value);
        self
    }

    /// Add an include entry
    pub fn with_include(mut self, include: IncludeEntry) -> Self {
        self.includes.push(include);
        self
    }
}

/// A fully assembled dimension instance: defaults gap-filled, wrapped in
/// sections (`meta`, plus any custom section), with a resolved parent chain.
#[derive(Debug, Clone)]
pub struct Dimension {
    /// Dimension type (e.g., "env")
    pub dim_type: DimType,
    /// Dimension name (e.g., "prod")
    pub name: String,
    /// Section data (key-value pairs), always contains at least "meta" and "name"
    pub data: HashMap<String, Value>,
    /// Non-JSON includes (files/folders) attached to this dimension
    pub includes: Vec<IncludeEntry>,
    /// Parent dimension reference (e.g., "dome:prod"), read from meta.parent
    pub parent_ref: Option<String>,
    /// Full chain of "type:name" from root ancestor to this dimension (inclusive)
    pub key_path: Vec<String>,
    /// SHA-256 of the canonical (key-ordered) JSON of `data`
    pub data_sha: Option<String>,
    /// Child dimension refs ("type:name"), populated by the application layer
    /// (requires scanning sibling dimensions - not something the domain can do
    /// on its own without a repository).
    pub kids: Vec<String>,
}

impl Dimension {
    /// Create a new dimension (bare constructor, mostly useful for tests/builders)
    pub fn new(dim_type: impl Into<DimType>, name: impl Into<String>) -> Self {
        Self {
            dim_type: dim_type.into(),
            name: name.into(),
            data: HashMap::new(),
            includes: Vec::new(),
            parent_ref: None,
            key_path: Vec::new(),
            data_sha: None,
            kids: Vec::new(),
        }
    }

    /// Create dimension with data
    pub fn with_data(mut self, data: HashMap<String, Value>) -> Self {
        self.data = data;
        self
    }

    /// Set parent reference
    pub fn with_parent_ref(mut self, parent_ref: impl Into<String>) -> Self {
        self.parent_ref = Some(parent_ref.into());
        self
    }

    /// Set the key path explicitly
    pub fn with_key_path(mut self, key_path: Vec<String>) -> Self {
        self.key_path = key_path;
        self
    }

    /// Set resolved child refs
    pub fn with_kids(mut self, kids: Vec<String>) -> Self {
        self.kids = kids;
        self
    }

    /// Assemble a full [`Dimension`] from a [`RawDimension`], gap-filling
    /// defaults and resolving the parent chain. Pure function: no I/O, no
    /// panics. `parent` must already be a fully assembled Dimension (the
    /// caller/service is responsible for recursively resolving it first).
    pub fn assemble(
        dim_type: impl Into<DimType>,
        raw: RawDimension,
        defaults: Option<&RawDimension>,
        parent: Option<&Dimension>,
    ) -> Dimension {
        let dim_type = dim_type.into();
        let mut data = raw.sections;

        if let Some(defaults) = defaults {
            for (section, default_value) in &defaults.sections {
                match data.get_mut(section) {
                    Some(existing) => gap_fill_merge(existing, default_value),
                    None => {
                        data.insert(section.clone(), default_value.clone());
                    }
                }
            }
        }

        // "meta" always exists once assembled, even if empty.
        data.entry("meta".to_string())
            .or_insert_with(|| Value::Object(Default::default()));
        data.insert("name".to_string(), Value::String(raw.name.clone()));

        let parent_ref = data
            .get("meta")
            .and_then(|m| m.get("parent"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut key_path = match parent {
            Some(p) => p.key_path.clone(),
            None => Vec::new(),
        };
        key_path.push(format!("{}:{}", dim_type, raw.name));

        let data_sha = Some(sha256_of_sections(&data));

        Dimension {
            dim_type,
            name: raw.name,
            data,
            includes: raw.includes,
            parent_ref,
            key_path,
            data_sha,
            kids: Vec::new(),
        }
    }

    /// Get the dimension key (type:name)
    pub fn key(&self) -> String {
        format!("{}:{}", self.dim_type, self.name)
    }

    /// Parse a dimension key string into type and name
    pub fn parse_key(key: &str) -> DomainResult<(DimType, String)> {
        let parts: Vec<&str> = key.splitn(2, ':').collect();
        if parts.len() != 2 {
            return Err(DomainError::InvalidDimensionFormat {
                input: key.to_string(),
                expected: "<dim_type>:<dim_name>",
            });
        }
        Ok((DimType::new(parts[0]), parts[1].to_string()))
    }

    /// Get a value from dimension data
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Get meta data (commonly stored under "meta" key)
    pub fn meta(&self) -> Option<&Value> {
        self.data.get("meta")
    }

    /// Get a named section (e.g. "manifest")
    pub fn section(&self, name: &str) -> Option<&Value> {
        self.data.get(name)
    }

    /// Render all sections as a single JSON object (used for flattening into
    /// `cubtera_dim_{type}.json`, API responses, etc.)
    pub fn to_json(&self) -> Value {
        Value::Object(
            self.data
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
        )
    }

    /// Render this dimension as a full resource: its section data plus
    /// resolution metadata that isn't itself part of any section (`name`,
    /// `type`, `parent`, `key_path`, `data_sha`, `kids`). Shared shape for
    /// every interface layer (REST API, MCP) so responses stay consistent.
    pub fn to_response_json(&self) -> Value {
        let mut obj = match self.to_json() {
            Value::Object(map) => map,
            _ => serde_json::Map::new(),
        };
        obj.insert("name".to_string(), json!(self.name));
        obj.insert("type".to_string(), json!(self.dim_type.as_str()));
        obj.insert("parent".to_string(), json!(self.parent_ref));
        obj.insert("key_path".to_string(), json!(self.key_path));
        obj.insert("data_sha".to_string(), json!(self.data_sha));
        obj.insert("kids".to_string(), json!(self.kids));
        Value::Object(obj)
    }

    /// Full "type:name" chain from root ancestor to self, e.g.
    /// `["dome:prod", "env:prod", "dc:us-east-1"]`
    pub fn dim_tree(&self) -> &[String] {
        &self.key_path
    }
}

/// Gap-fill merge: keys present in `data` win; keys only in `defaults` are
/// copied over; nested objects are merged recursively. Arrays and scalars in
/// `data` are never touched.
pub fn gap_fill_merge(data: &mut Value, defaults: &Value) {
    if let (Value::Object(data_obj), Value::Object(defaults_obj)) = (data, defaults) {
        for (key, default_value) in defaults_obj {
            match data_obj.get_mut(key) {
                None => {
                    data_obj.insert(key.clone(), default_value.clone());
                }
                Some(existing) if existing.is_object() && default_value.is_object() => {
                    gap_fill_merge(existing, default_value);
                }
                Some(_) => {} // data's value takes priority
            }
        }
    }
}

/// SHA-256 hex digest of the canonical (key-ordered) JSON representation of a
/// sections map. Deterministic regardless of HashMap iteration order.
pub fn sha256_of_sections(sections: &HashMap<String, Value>) -> String {
    let value = Value::Object(
        sections
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect(),
    );
    sha256_of_value(&value)
}

/// SHA-256 hex digest of the canonical (key-ordered) JSON representation of a value.
pub fn sha256_of_value(value: &Value) -> String {
    use sha2::Digest;
    let ordered = order_json(value);
    let canonical = serde_json::to_string(&ordered).unwrap_or_default();
    let mut hasher = sha2::Sha256::new();
    hasher.update(canonical.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn order_json(value: &Value) -> Value {
    use std::collections::BTreeMap;
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

/// Dimension hierarchy configuration
#[derive(Debug, Clone)]
pub struct DimHierarchy {
    /// Ordered list of dimension types (parent to child)
    relations: Vec<DimType>,
}

impl DimHierarchy {
    /// Create a new hierarchy from dimension type names
    pub fn new(relations: Vec<impl Into<DimType>>) -> Self {
        Self {
            relations: relations.into_iter().map(Into::into).collect(),
        }
    }

    /// Get the parent dimension type for a given type
    pub fn parent_type(&self, dim_type: &DimType) -> Option<&DimType> {
        let pos = self.relations.iter().position(|t| t == dim_type)?;
        if pos == 0 {
            None
        } else {
            Some(&self.relations[pos - 1])
        }
    }

    /// Get the child dimension type for a given type
    pub fn child_type(&self, dim_type: &DimType) -> Option<&DimType> {
        let pos = self.relations.iter().position(|t| t == dim_type)?;
        self.relations.get(pos + 1)
    }

    /// Check if a dimension type is in the hierarchy
    pub fn contains(&self, dim_type: &DimType) -> bool {
        self.relations.contains(dim_type)
    }

    /// Get all dimension types in order
    pub fn types(&self) -> &[DimType] {
        &self.relations
    }
}

impl Default for DimHierarchy {
    fn default() -> Self {
        Self::new(vec!["dome", "env", "dc"])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_dim_type_creation() {
        let dt = DimType::new("env");
        assert_eq!(dt.as_str(), "env");
    }

    #[test]
    fn test_dimension_key() {
        let dim = Dimension::new("env", "prod");
        assert_eq!(dim.key(), "env:prod");
    }

    #[test]
    fn test_parse_key_valid() {
        let (dim_type, name) = Dimension::parse_key("env:prod").unwrap();
        assert_eq!(dim_type.as_str(), "env");
        assert_eq!(name, "prod");
    }

    #[test]
    fn test_parse_key_invalid() {
        let result = Dimension::parse_key("invalid");
        assert!(result.is_err());
    }

    #[test]
    fn test_hierarchy_parent() {
        let hierarchy = DimHierarchy::default();
        let env_type = DimType::new("env");
        let parent = hierarchy.parent_type(&env_type);
        assert_eq!(parent.map(|t| t.as_str()), Some("dome"));
    }

    #[test]
    fn test_hierarchy_child() {
        let hierarchy = DimHierarchy::default();
        let env_type = DimType::new("env");
        let child = hierarchy.child_type(&env_type);
        assert_eq!(child.map(|t| t.as_str()), Some("dc"));
    }

    #[test]
    fn test_hierarchy_no_parent_for_root() {
        let hierarchy = DimHierarchy::default();
        let dome_type = DimType::new("dome");
        assert!(hierarchy.parent_type(&dome_type).is_none());
    }

    #[test]
    fn test_gap_fill_merge_data_priority() {
        let mut data = json!({"a": 1, "nested": {"x": 1}});
        let defaults = json!({"a": 2, "b": 2, "nested": {"x": 2, "y": 2}});
        gap_fill_merge(&mut data, &defaults);
        assert_eq!(data["a"], 1); // data wins
        assert_eq!(data["b"], 2); // filled from defaults
        assert_eq!(data["nested"]["x"], 1); // nested data wins
        assert_eq!(data["nested"]["y"], 2); // nested gap-filled
    }

    #[test]
    fn test_assemble_wraps_meta_and_sets_name() {
        let raw =
            RawDimension::new("stg1-use2").with_section("meta", json!({"region": "us-east-2"}));
        let dim = Dimension::assemble("dc", raw, None, None);
        assert_eq!(dim.name, "stg1-use2");
        assert_eq!(dim.meta().unwrap()["region"], "us-east-2");
        assert_eq!(dim.data["name"], "stg1-use2");
        assert_eq!(dim.key_path, vec!["dc:stg1-use2".to_string()]);
        assert!(dim.data_sha.is_some());
    }

    #[test]
    fn test_assemble_gap_fills_defaults_per_section() {
        let raw = RawDimension::new("stg1").with_section("meta", json!({"region": "us-east-2"}));
        let defaults = RawDimension::new(".default").with_section(
            "meta",
            json!({"region": "us-east-1", "vpc_cidr": "10.0.0.0/16"}),
        );
        let dim = Dimension::assemble("dc", raw, Some(&defaults), None);
        assert_eq!(dim.meta().unwrap()["region"], "us-east-2"); // own wins
        assert_eq!(dim.meta().unwrap()["vpc_cidr"], "10.0.0.0/16"); // gap-filled
    }

    #[test]
    fn test_assemble_parent_chain() {
        let root_raw = RawDimension::new("prod").with_section("meta", json!({}));
        let root = Dimension::assemble("dome", root_raw, None, None);

        let child_raw =
            RawDimension::new("prod").with_section("meta", json!({"parent": "dome:prod"}));
        let child = Dimension::assemble("env", child_raw, None, Some(&root));

        assert_eq!(child.parent_ref, Some("dome:prod".to_string()));
        assert_eq!(
            child.key_path,
            vec!["dome:prod".to_string(), "env:prod".to_string()]
        );
    }

    #[test]
    fn test_sha256_of_sections_is_order_independent() {
        let mut a = HashMap::new();
        a.insert("b".to_string(), json!(1));
        a.insert("a".to_string(), json!(2));

        let mut b = HashMap::new();
        b.insert("a".to_string(), json!(2));
        b.insert("b".to_string(), json!(1));

        assert_eq!(sha256_of_sections(&a), sha256_of_sections(&b));
    }

    #[test]
    fn test_to_json_roundtrips_sections() {
        let dim = Dimension::new("env", "prod").with_data(HashMap::from([
            ("meta".to_string(), json!({"prod": true})),
            ("name".to_string(), json!("prod")),
        ]));
        let value = dim.to_json();
        assert_eq!(value["meta"]["prod"], true);
        assert_eq!(value["name"], "prod");
    }
}
