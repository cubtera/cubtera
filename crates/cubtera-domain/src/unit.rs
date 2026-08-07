//! Unit entity and related types
//!
//! A Unit represents an atomic infrastructure operation.

use crate::dimension::{DimType, Dimension};
use crate::error::{DomainError, DomainResult};
use crate::manifest::Manifest;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
    /// Dimension data (type -> data JSON)
    pub dimension_data: HashMap<String, Value>,
    /// Source path (unit directory)
    pub unit_path: PathBuf,
    /// Temp folder for runner execution
    pub temp_folder: PathBuf,
    /// Extensions (additional dimension-like parameters)
    pub extensions: Vec<String>,
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
            dimension_data: HashMap::new(),
            unit_path: PathBuf::new(),
            temp_folder: PathBuf::new(),
            extensions: Vec::new(),
            git_sha: None,
        }
    }

    /// Add a dimension reference
    pub fn with_dimension(mut self, dim_ref: DimensionRef) -> Self {
        self.dimensions.push(dim_ref);
        self
    }

    /// Set unit source path
    pub fn with_unit_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.unit_path = path.into();
        self
    }

    /// Set temp folder path
    pub fn with_temp_folder(mut self, path: impl Into<PathBuf>) -> Self {
        self.temp_folder = path.into();
        self
    }

    /// Add extensions
    pub fn with_extensions(mut self, extensions: Vec<String>) -> Self {
        self.extensions = extensions;
        self
    }

    /// Add dimension data for a dimension type
    pub fn with_dimension_data(mut self, dim_type: impl Into<String>, data: Value) -> Self {
        self.dimension_data.insert(dim_type.into(), data);
        self
    }

    /// Set all dimension data at once
    pub fn with_all_dimension_data(mut self, data: HashMap<String, Value>) -> Self {
        self.dimension_data = data;
        self
    }

    /// Calculate temp folder path based on org, unit name, dimensions and extensions
    pub fn calculate_temp_folder(&self, base_temp_path: &Path) -> PathBuf {
        let mut path = base_temp_path.join(&self.org).join(&self.name);
        
        // Add dimensions to path
        for dim in &self.dimensions {
            path = path.join(dim.key());
        }
        
        // Add extensions to path
        for ext in &self.extensions {
            path = path.join(ext);
        }
        
        path
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
        let mut parts: Vec<String> = self.dimensions.iter().map(|d| d.key()).collect();
        parts.extend(self.extensions.clone());
        parts.join("/")
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

    /// Remove temp folder if it exists
    pub fn remove_temp_folder(&self) -> DomainResult<()> {
        if self.temp_folder.exists() {
            std::fs::remove_dir_all(&self.temp_folder).map_err(|e| {
                DomainError::io(format!(
                    "Failed to remove temp folder {:?}: {}",
                    self.temp_folder, e
                ))
            })?;
        }
        Ok(())
    }

    /// Check if temp folder exists
    pub fn temp_folder_exists(&self) -> bool {
        self.temp_folder.exists()
    }

    /// Copy unit files to temp folder
    pub fn copy_files_to_temp(
        &self,
        modules_path: &Path,
        plugins_path: &Path,
    ) -> DomainResult<()> {
        let dest = &self.temp_folder;

        // Create temp folder if not exists
        if !dest.exists() {
            std::fs::create_dir_all(dest).map_err(|e| {
                DomainError::io(format!("Failed to create temp folder {:?}: {}", dest, e))
            })?;
        }

        // Create modules symlink
        let modules_link = dest.join("modules");
        if !modules_link.exists() && modules_path.exists() {
            #[cfg(unix)]
            std::os::unix::fs::symlink(modules_path, &modules_link).map_err(|e| {
                DomainError::io(format!("Failed to create modules symlink: {}", e))
            })?;
        }

        // Copy plugins to ~/.terraform.d/plugins (if plugins_path exists)
        if plugins_path.exists() {
            if let Some(home) = std::env::var("HOME").ok() {
                let tf_plugins = PathBuf::from(home).join(".terraform.d/plugins");
                if !tf_plugins.exists() {
                    let _ = std::fs::create_dir_all(&tf_plugins);
                }
                copy_dir_contents(plugins_path, &tf_plugins)?;
            }
        }

        // Copy unit files to temp folder
        if self.unit_path.exists() {
            copy_dir_contents(&self.unit_path, dest)?;
        }

        // Write dimension data as cubtera_dim_{type}.json files
        self.write_dimension_data(dest)?;

        Ok(())
    }

    /// Write dimension data to temp folder as cubtera_dim_{type}.json files
    pub fn write_dimension_data(&self, dest: &Path) -> DomainResult<()> {
        for dim_ref in &self.dimensions {
            let dim_type = dim_ref.dim_type.as_str();
            let dim_name = &dim_ref.name;

            // Build the dimension vars JSON
            let mut vars = serde_json::Map::new();
            vars.insert(
                format!("dim_{}_name", dim_type),
                Value::String(dim_name.clone()),
            );

            // Add dimension data if available
            if let Some(data) = self.dimension_data.get(dim_type) {
                // Flatten data into dim_{type}_{key} format
                if let Some(obj) = data.as_object() {
                    for (key, value) in obj {
                        vars.insert(format!("dim_{}_{}", dim_type, key), value.clone());
                    }
                }
                // Also add the full data as dim_{type}_meta
                vars.insert(format!("dim_{}_meta", dim_type), data.clone());
            }

            // Write to file
            let filename = format!("cubtera_dim_{}.json", dim_type);
            let filepath = dest.join(&filename);
            let json_content = serde_json::to_string_pretty(&Value::Object(vars))
                .map_err(|e| DomainError::io(format!("Failed to serialize dimension data: {}", e)))?;

            std::fs::write(&filepath, json_content)
                .map_err(|e| DomainError::io(format!("Failed to write dimension file {:?}: {}", filepath, e)))?;
        }

        Ok(())
    }
}

/// Copy directory contents recursively
fn copy_dir_contents(src: &Path, dst: &Path) -> DomainResult<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst).map_err(|e| {
            DomainError::io(format!("Failed to create directory {:?}: {}", dst, e))
        })?;
    }

    for entry in std::fs::read_dir(src).map_err(|e| {
        DomainError::io(format!("Failed to read directory {:?}: {}", src, e))
    })? {
        let entry = entry.map_err(|e| {
            DomainError::io(format!("Failed to read directory entry: {}", e))
        })?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_contents(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                DomainError::io(format!(
                    "Failed to copy {:?} to {:?}: {}",
                    src_path, dst_path, e
                ))
            })?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::manifest::RunnerType;
    use std::path::PathBuf;

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
    fn test_unit_dim_tree_with_extensions() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_extensions(vec!["index:0".to_string(), "replica:1".to_string()]);

        assert_eq!(unit.dim_tree(), "dome:prod/index:0/replica:1");
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

    #[test]
    fn test_calculate_temp_folder() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_dimension(DimensionRef::new("dome", "prod"))
            .with_dimension(DimensionRef::new("env", "stg"))
            .with_extensions(vec!["index:0".to_string()]);

        let temp_folder = unit.calculate_temp_folder(&PathBuf::from("/tmp/cubtera"));
        assert_eq!(
            temp_folder,
            PathBuf::from("/tmp/cubtera/cubtera/network/dome:prod/env:stg/index:0")
        );
    }

    #[test]
    fn test_temp_folder_exists_false() {
        let unit = Unit::new("network", "cubtera", create_test_manifest())
            .with_temp_folder("/nonexistent/path/that/does/not/exist");
        assert!(!unit.temp_folder_exists());
    }
}

