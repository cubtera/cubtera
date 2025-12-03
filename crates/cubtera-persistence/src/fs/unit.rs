//! File system unit repository

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::UnitRepository;
use cubtera_domain::{Manifest, RunnerType};
use std::fs;
use std::path::PathBuf;
use tracing::debug;

/// File system based unit repository
pub struct FsUnitRepository {
    units_path: PathBuf,
    org: String,
}

impl FsUnitRepository {
    /// Create a new FS unit repository
    pub fn new(units_path: PathBuf, org: String) -> Self {
        Self { units_path, org }
    }

    /// Find unit directory by name (searching recursively)
    fn find_unit_dir(&self, unit_name: &str) -> Option<PathBuf> {
        // First try direct path
        let direct_path = self.units_path.join(unit_name);
        if direct_path.exists() && direct_path.join("manifest.toml").exists() {
            return Some(direct_path);
        }

        // Search recursively
        self.search_unit_recursive(&self.units_path, unit_name)
    }

    fn search_unit_recursive(&self, dir: &PathBuf, unit_name: &str) -> Option<PathBuf> {
        if !dir.exists() || !dir.is_dir() {
            return None;
        }

        let entries = fs::read_dir(dir).ok()?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = path.file_name()?.to_str()?;
                if name == unit_name && path.join("manifest.toml").exists() {
                    return Some(path);
                }
                // Recurse into subdirectories
                if let Some(found) = self.search_unit_recursive(&path, unit_name) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Parse manifest from TOML file
    fn parse_manifest(&self, content: &str) -> AppResult<Manifest> {
        let toml_value: toml::Value =
            toml::from_str(content).map_err(|e| AppError::repository(e.to_string()))?;

        let mut manifest = Manifest::default();

        // Parse dimensions
        if let Some(dims) = toml_value.get("dimensions").and_then(|v| v.as_array()) {
            manifest.dimensions = dims
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }

        // Parse optional dimensions
        if let Some(dims) = toml_value.get("optDims").and_then(|v| v.as_array()) {
            manifest.optional_dimensions = dims
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }

        // Parse runner type
        if let Some(runner_type) = toml_value.get("type").and_then(|v| v.as_str()) {
            manifest.runner_type = RunnerType::from_str(runner_type);
        }

        // Parse overwrite
        if let Some(overwrite) = toml_value.get("overwrite").and_then(|v| v.as_bool()) {
            manifest.overwrite = overwrite;
        }

        // Parse allow list
        if let Some(list) = toml_value.get("allowList").and_then(|v| v.as_array()) {
            manifest.allow_list = Some(
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
            );
        }

        // Parse deny list
        if let Some(list) = toml_value.get("denyList").and_then(|v| v.as_array()) {
            manifest.deny_list = Some(
                list.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect(),
            );
        }

        // Parse runner config
        if let Some(runner) = toml_value.get("runner").and_then(|v| v.as_table()) {
            for (k, v) in runner {
                if let Some(s) = v.as_str() {
                    manifest.runner_config.insert(k.clone(), s.to_string());
                } else {
                    manifest
                        .runner_config
                        .insert(k.clone(), v.to_string());
                }
            }
        }

        // Parse state config
        if let Some(state) = toml_value.get("state").and_then(|v| v.as_table()) {
            for (k, v) in state {
                if let Some(s) = v.as_str() {
                    manifest.state_config.insert(k.clone(), s.to_string());
                } else {
                    manifest.state_config.insert(k.clone(), v.to_string());
                }
            }
        }

        Ok(manifest)
    }
}

#[async_trait]
impl UnitRepository for FsUnitRepository {
    async fn find_manifest(&self, _org: &str, unit_name: &str) -> AppResult<Option<Manifest>> {
        let unit_dir = match self.find_unit_dir(unit_name) {
            Some(dir) => dir,
            None => return Ok(None),
        };

        let manifest_path = unit_dir.join("manifest.toml");
        debug!("Loading manifest from: {:?}", manifest_path);

        if !manifest_path.exists() {
            return Ok(None);
        }

        let content = fs::read_to_string(&manifest_path).map_err(|e| AppError::io(e.to_string()))?;
        let manifest = self.parse_manifest(&content)?;

        Ok(Some(manifest))
    }

    async fn get_unit_path(&self, _org: &str, unit_name: &str) -> AppResult<Option<String>> {
        Ok(self
            .find_unit_dir(unit_name)
            .map(|p| p.to_string_lossy().to_string()))
    }

    async fn list_units(&self, _org: &str) -> AppResult<Vec<String>> {
        let mut units = Vec::new();
        self.collect_units(&self.units_path, &mut units)?;
        Ok(units)
    }
}

impl FsUnitRepository {
    fn collect_units(&self, dir: &PathBuf, units: &mut Vec<String>) -> AppResult<()> {
        if !dir.exists() || !dir.is_dir() {
            return Ok(());
        }

        let entries = fs::read_dir(dir).map_err(|e| AppError::io(e.to_string()))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("manifest.toml").exists() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        units.push(name.to_string());
                    }
                }
                // Recurse
                self.collect_units(&path, units)?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_find_manifest_not_found() {
        let temp_dir = tempdir().unwrap();
        let repo = FsUnitRepository::new(temp_dir.path().to_path_buf(), "test".to_string());

        let result = repo.find_manifest("test", "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_find_manifest_exists() {
        let temp_dir = tempdir().unwrap();
        let unit_dir = temp_dir.path().join("my_unit");
        fs::create_dir_all(&unit_dir).unwrap();

        let manifest_content = r#"
dimensions = ["env", "dc"]
type = "tf"

[runner]
version = "1.5.0"
"#;
        fs::write(unit_dir.join("manifest.toml"), manifest_content).unwrap();

        let repo = FsUnitRepository::new(temp_dir.path().to_path_buf(), "test".to_string());

        let result = repo.find_manifest("test", "my_unit").await.unwrap();
        assert!(result.is_some());

        let manifest = result.unwrap();
        assert_eq!(manifest.dimensions, vec!["env", "dc"]);
        assert_eq!(manifest.runner_type, RunnerType::Terraform);
    }

    #[tokio::test]
    async fn test_list_units() {
        let temp_dir = tempdir().unwrap();

        // Create two units
        for name in &["unit1", "unit2"] {
            let unit_dir = temp_dir.path().join(name);
            fs::create_dir_all(&unit_dir).unwrap();
            fs::write(unit_dir.join("manifest.toml"), "dimensions = []").unwrap();
        }

        let repo = FsUnitRepository::new(temp_dir.path().to_path_buf(), "test".to_string());
        let units = repo.list_units("test").await.unwrap();

        assert_eq!(units.len(), 2);
        assert!(units.contains(&"unit1".to_string()));
        assert!(units.contains(&"unit2".to_string()));
    }
}

