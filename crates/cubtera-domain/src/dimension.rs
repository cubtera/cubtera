//! Dimension entity and related types
//!
//! A Dimension represents a logical grouping for infrastructure organization,
//! such as environment, region, or data center.

use crate::error::{DomainError, DomainResult};
use crate::value::Value;
use std::collections::HashMap;

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

/// A dimension instance with its data
#[derive(Debug, Clone)]
pub struct Dimension {
    /// Dimension type (e.g., "env")
    pub dim_type: DimType,
    /// Dimension name (e.g., "prod")
    pub name: String,
    /// Dimension data (key-value pairs)
    pub data: HashMap<String, Value>,
    /// Parent dimension reference (e.g., "dome:prod")
    pub parent_ref: Option<String>,
    /// SHA hash of the data for change detection
    pub data_sha: Option<String>,
}

impl Dimension {
    /// Create a new dimension
    pub fn new(dim_type: impl Into<DimType>, name: impl Into<String>) -> Self {
        Self {
            dim_type: dim_type.into(),
            name: name.into(),
            data: HashMap::new(),
            parent_ref: None,
            data_sha: None,
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
}

