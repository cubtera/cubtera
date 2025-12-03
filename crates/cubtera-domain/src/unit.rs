//! Unit entity and related types
//!
//! A Unit represents an atomic infrastructure operation.

use crate::dimension::{DimType, Dimension};
use crate::manifest::Manifest;

/// A unit of infrastructure operation
#[derive(Debug, Clone)]
pub struct Unit {
    /// Unit name
    pub name: String,
    /// Organization name
    pub org: String,
    /// Unit manifest
    pub manifest: Manifest,
    /// Resolved dimensions for this unit
    pub dimensions: Vec<DimensionRef>,
    /// Source path (unit directory)
    pub source_path: Option<String>,
    /// Git SHA of the unit source
    pub git_sha: Option<String>,
}

/// Reference to a dimension value
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DimensionRef {
    /// Dimension type
    pub dim_type: DimType,
    /// Dimension name
    pub name: String,
}

impl DimensionRef {
    /// Create a new dimension reference
    pub fn new(dim_type: impl Into<DimType>, name: impl Into<String>) -> Self {
        Self {
            dim_type: dim_type.into(),
            name: name.into(),
        }
    }

    /// Parse from string (format: "type:name")
    pub fn parse(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.splitn(2, ':').collect();
        if parts.len() == 2 {
            Some(Self::new(parts[0], parts[1]))
        } else {
            None
        }
    }

    /// Get the key representation (type:name)
    pub fn key(&self) -> String {
        format!("{}:{}", self.dim_type, self.name)
    }
}

impl From<&Dimension> for DimensionRef {
    fn from(dim: &Dimension) -> Self {
        Self::new(dim.dim_type.clone(), dim.name.clone())
    }
}

impl Unit {
    /// Create a new unit
    pub fn new(name: impl Into<String>, org: impl Into<String>, manifest: Manifest) -> Self {
        Self {
            name: name.into(),
            org: org.into(),
            manifest,
            dimensions: Vec::new(),
            source_path: None,
            git_sha: None,
        }
    }

    /// Add a dimension reference
    pub fn with_dimension(mut self, dim_ref: DimensionRef) -> Self {
        self.dimensions.push(dim_ref);
        self
    }

    /// Set source path
    pub fn with_source_path(mut self, path: impl Into<String>) -> Self {
        self.source_path = Some(path.into());
        self
    }

    /// Calculate the state path based on dimensions
    pub fn state_path(&self) -> String {
        let dim_parts: Vec<String> = self.dimensions.iter().map(|d| d.key()).collect();
        if dim_parts.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", dim_parts.join("/"), self.name)
        }
    }

    /// Calculate the dimension tree (for state path templates)
    pub fn dim_tree(&self) -> String {
        self.dimensions.iter().map(|d| d.key()).collect::<Vec<_>>().join("/")
    }

    /// Get dimension reference by type
    pub fn get_dimension(&self, dim_type: &str) -> Option<&DimensionRef> {
        self.dimensions.iter().find(|d| d.dim_type.as_str() == dim_type)
    }

    /// Check if unit has all required dimensions from manifest
    pub fn has_all_required_dimensions(&self) -> bool {
        self.manifest.dimensions.iter().all(|required| {
            self.dimensions.iter().any(|d| d.dim_type.as_str() == required)
        })
    }

    /// Get missing required dimensions
    pub fn missing_dimensions(&self) -> Vec<&str> {
        self.manifest
            .dimensions
            .iter()
            .filter(|required| {
                !self.dimensions.iter().any(|d| d.dim_type.as_str() == required.as_str())
            })
            .map(|s| s.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RunnerType;

    fn create_test_manifest() -> Manifest {
        Manifest::new(
            vec!["dome".to_string(), "env".to_string(), "dc".to_string()],
            RunnerType::Terraform,
        )
    }

    #[test]
    fn test_dimension_ref_parse() {
        let dim_ref = DimensionRef::parse("env:prod").unwrap();
        assert_eq!(dim_ref.dim_type.as_str(), "env");
        assert_eq!(dim_ref.name, "prod");
    }

    #[test]
    fn test_dimension_ref_key() {
        let dim_ref = DimensionRef::new("env", "prod");
        assert_eq!(dim_ref.key(), "env:prod");
    }

    #[test]
    fn test_unit_state_path() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension(DimensionRef::new("dc", "us-east-1"));

        assert_eq!(
            unit.state_path(),
            "dome:prod/env:prod/dc:us-east-1/network"
        );
    }

    #[test]
    fn test_unit_dim_tree() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"));

        assert_eq!(unit.dim_tree(), "dome:prod/env:prod");
    }

    #[test]
    fn test_unit_missing_dimensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"));

        let missing = unit.missing_dimensions();
        assert!(missing.contains(&"env"));
        assert!(missing.contains(&"dc"));
        assert!(!missing.contains(&"dome"));
    }

    #[test]
    fn test_unit_has_all_dimensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "prod"))
            .with_dimension(DimensionRef::new("dc", "us-east-1"));

        assert!(unit.has_all_required_dimensions());
    }
}

