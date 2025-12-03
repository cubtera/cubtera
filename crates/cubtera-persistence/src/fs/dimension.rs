//! File system dimension repository

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::DimensionRepository;
use cubtera_domain::{DimType, Dimension, Value};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// File system based dimension repository
pub struct FsDimensionRepository {
    base_path: PathBuf,
    org: String,
}

impl FsDimensionRepository {
    /// Create a new FS dimension repository
    pub fn new(base_path: PathBuf, org: String) -> Self {
        Self { base_path, org }
    }

    /// Get the path for a dimension type directory
    fn dim_type_path(&self, org: &str, dim_type: &DimType) -> PathBuf {
        self.base_path.join(org).join(dim_type.as_str())
    }

    /// Get the path for a specific dimension file
    fn dim_file_path(&self, org: &str, dim_type: &DimType, name: &str) -> PathBuf {
        self.dim_type_path(org, dim_type).join(format!("{}.json", name))
    }

    /// Get the path for defaults file
    fn defaults_path(&self, org: &str, dim_type: &DimType) -> PathBuf {
        self.dim_type_path(org, dim_type)
            .join(".defaults.json")
    }

    /// Load dimension from JSON file
    fn load_dimension(&self, path: &Path, dim_type: &DimType) -> AppResult<Option<Dimension>> {
        if !path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(path).map_err(|e| AppError::io(e.to_string()))?;
        let data: serde_json::Value =
            serde_json::from_str(&content).map_err(|e| AppError::repository(e.to_string()))?;

        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default()
            .to_string();

        // Convert serde_json::Value to domain Value
        let domain_data = self.convert_json_to_domain(&data)?;

        let parent_ref = data
            .get("parent")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let mut dim = Dimension::new(dim_type.clone(), name);
        if let Value::Object(obj) = domain_data {
            dim = dim.with_data(obj);
        }
        if let Some(parent) = parent_ref {
            dim = dim.with_parent_ref(parent);
        }

        Ok(Some(dim))
    }

    /// Convert serde_json::Value to domain Value
    fn convert_json_to_domain(&self, json: &serde_json::Value) -> AppResult<Value> {
        match json {
            serde_json::Value::Null => Ok(Value::Null),
            serde_json::Value::Bool(b) => Ok(Value::Bool(*b)),
            serde_json::Value::Number(n) => {
                Ok(Value::Number(n.as_f64().unwrap_or(0.0)))
            }
            serde_json::Value::String(s) => Ok(Value::String(s.clone())),
            serde_json::Value::Array(arr) => {
                let items: Result<Vec<Value>, _> = arr
                    .iter()
                    .map(|v| self.convert_json_to_domain(v))
                    .collect();
                Ok(Value::Array(items?))
            }
            serde_json::Value::Object(obj) => {
                let mut map = HashMap::new();
                for (k, v) in obj {
                    map.insert(k.clone(), self.convert_json_to_domain(v)?);
                }
                Ok(Value::Object(map))
            }
        }
    }
}

#[async_trait]
impl DimensionRepository for FsDimensionRepository {
    async fn find_by_name(
        &self,
        org: &str,
        dim_type: &DimType,
        name: &str,
    ) -> AppResult<Option<Dimension>> {
        let path = self.dim_file_path(org, dim_type, name);
        debug!("Loading dimension from: {:?}", path);
        self.load_dimension(&path, dim_type)
    }

    async fn find_all(&self, org: &str, dim_type: &DimType) -> AppResult<Vec<Dimension>> {
        let dir_path = self.dim_type_path(org, dim_type);
        if !dir_path.exists() {
            return Ok(Vec::new());
        }

        let mut dimensions = Vec::new();
        let entries = fs::read_dir(&dir_path).map_err(|e| AppError::io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();

            if path.extension().map(|e| e == "json").unwrap_or(false) {
                // Skip defaults and manifest files
                if let Some(name) = path.file_stem().and_then(|s| s.to_str()) {
                    if name.starts_with('.') || name.contains(':') {
                        continue;
                    }
                }

                if let Some(dim) = self.load_dimension(&path, dim_type)? {
                    dimensions.push(dim);
                }
            }
        }

        Ok(dimensions)
    }

    async fn find_names(&self, org: &str, dim_type: &DimType) -> AppResult<Vec<String>> {
        let dims = self.find_all(org, dim_type).await?;
        Ok(dims.into_iter().map(|d| d.name).collect())
    }

    async fn find_defaults(
        &self,
        org: &str,
        dim_type: &DimType,
    ) -> AppResult<Option<Dimension>> {
        let path = self.defaults_path(org, dim_type);
        self.load_dimension(&path, dim_type)
    }

    async fn find_children(
        &self,
        org: &str,
        parent_type: &DimType,
        parent_name: &str,
    ) -> AppResult<Vec<Dimension>> {
        // This requires knowing the child type from hierarchy
        // For now, search all types and filter by parent
        let parent_ref = format!("{}:{}", parent_type, parent_name);
        let dim_types = self.get_dim_types(org).await?;

        let mut children = Vec::new();
        for type_name in dim_types {
            let dim_type = DimType::new(&type_name);
            let dims = self.find_all(org, &dim_type).await?;
            for dim in dims {
                if dim.parent_ref.as_ref() == Some(&parent_ref) {
                    children.push(dim);
                }
            }
        }

        Ok(children)
    }

    async fn find_parent(
        &self,
        org: &str,
        dim_type: &DimType,
        name: &str,
    ) -> AppResult<Option<Dimension>> {
        let dim = self.find_by_name(org, dim_type, name).await?;
        if let Some(dim) = dim {
            if let Some(parent_ref) = &dim.parent_ref {
                let (parent_type, parent_name) = Dimension::parse_key(parent_ref)?;
                return self.find_by_name(org, &parent_type, &parent_name).await;
            }
        }
        Ok(None)
    }

    async fn get_dim_types(&self, org: &str) -> AppResult<Vec<String>> {
        let org_path = self.base_path.join(org);
        if !org_path.exists() {
            return Ok(Vec::new());
        }

        let mut types = Vec::new();
        let entries = fs::read_dir(&org_path).map_err(|e| AppError::io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    types.push(name.to_string());
                }
            }
        }

        Ok(types)
    }

    async fn get_orgs(&self) -> AppResult<Vec<String>> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }

        let mut orgs = Vec::new();
        let entries = fs::read_dir(&self.base_path).map_err(|e| AppError::io(e.to_string()))?;

        for entry in entries {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    orgs.push(name.to_string());
                }
            }
        }

        Ok(orgs)
    }

    async fn save(&self, org: &str, dimension: &Dimension) -> AppResult<()> {
        let path = self.dim_file_path(org, &dimension.dim_type, &dimension.name);

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| AppError::io(e.to_string()))?;
        }

        // Convert domain Value to serde_json::Value and save
        // This is a simplified implementation
        let data = serde_json::json!({
            "name": dimension.name,
            "parent": dimension.parent_ref,
        });

        let content =
            serde_json::to_string_pretty(&data).map_err(|e| AppError::repository(e.to_string()))?;
        fs::write(&path, content).map_err(|e| AppError::io(e.to_string()))?;

        Ok(())
    }

    async fn delete(&self, org: &str, dim_type: &DimType, name: &str) -> AppResult<()> {
        let path = self.dim_file_path(org, dim_type, name);
        if path.exists() {
            fs::remove_file(&path).map_err(|e| AppError::io(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_find_by_name_not_found() {
        let temp_dir = tempdir().unwrap();
        let repo = FsDimensionRepository::new(temp_dir.path().to_path_buf(), "test".to_string());

        let result = repo
            .find_by_name("test", &DimType::new("env"), "prod")
            .await
            .unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_orgs_empty() {
        let temp_dir = tempdir().unwrap();
        let repo = FsDimensionRepository::new(temp_dir.path().to_path_buf(), "test".to_string());

        let orgs = repo.get_orgs().await.unwrap();
        assert!(orgs.is_empty());
    }
}

