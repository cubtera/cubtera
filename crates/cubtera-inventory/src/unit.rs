//! `FsUnitPort`: a from-scratch, v3-native port of v2's
//! `cubtera_persistence::fs::FsUnitRepository`, implementing
//! `cubtera_app::ports::UnitPort` directly - no dependency on
//! `cubtera-core`/`cubtera-persistence`.
//!
//! Units live somewhere under `unitsPath`, recursively - `manifest.toml`
//! marks a directory as a unit. `_org` is accepted but unused, matching
//! v2's behavior exactly: v1/v2 never partitioned units by org on disk.

use async_trait::async_trait;
use cubtera_app::ports::UnitPort;
use cubtera_app::{AppError, AppResult};
use cubtera_model::Manifest;
use std::fs;
use std::path::{Path, PathBuf};

/// FS-backed `UnitPort`, rooted at `<units_path>`.
pub struct FsUnitPort {
    units_path: PathBuf,
}

impl FsUnitPort {
    pub fn new(units_path: impl Into<PathBuf>) -> Self {
        Self {
            units_path: units_path.into(),
        }
    }
}

fn find_unit_dir(root: &Path, unit_name: &str) -> Option<PathBuf> {
    let direct_path = root.join(unit_name);
    if direct_path.exists() && direct_path.join("manifest.toml").exists() {
        return Some(direct_path);
    }
    search_unit_recursive(root, unit_name)
}

fn search_unit_recursive(dir: &Path, unit_name: &str) -> Option<PathBuf> {
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
            if let Some(found) = search_unit_recursive(&path, unit_name) {
                return Some(found);
            }
        }
    }
    None
}

fn collect_units(dir: &Path, units: &mut Vec<String>) -> AppResult<()> {
    if !dir.exists() || !dir.is_dir() {
        return Ok(());
    }

    let entries = fs::read_dir(dir).map_err(|e| AppError::backend(e.to_string()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("manifest.toml").exists() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    units.push(name.to_string());
                }
            }
            collect_units(&path, units)?;
        }
    }
    Ok(())
}

#[async_trait]
impl UnitPort for FsUnitPort {
    async fn find_manifest(&self, _org: &str, unit_name: &str) -> AppResult<Option<Manifest>> {
        let root = self.units_path.clone();
        let unit_name = unit_name.to_string();
        tokio::task::spawn_blocking(move || {
            let Some(unit_dir) = find_unit_dir(&root, &unit_name) else {
                return Ok(None);
            };
            let manifest_path = unit_dir.join("manifest.toml");
            let content = fs::read_to_string(&manifest_path)
                .map_err(|e| AppError::backend(format!("{manifest_path:?}: {e}")))?;
            let manifest = Manifest::from_toml(&content)?;
            Ok(Some(manifest))
        })
        .await
        .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }

    async fn get_unit_path(&self, _org: &str, unit_name: &str) -> AppResult<Option<String>> {
        let root = self.units_path.clone();
        let unit_name = unit_name.to_string();
        tokio::task::spawn_blocking(move || {
            find_unit_dir(&root, &unit_name).map(|p| p.to_string_lossy().to_string())
        })
        .await
        .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))
    }

    async fn list_units(&self, _org: &str) -> AppResult<Vec<String>> {
        let root = self.units_path.clone();
        tokio::task::spawn_blocking(move || {
            let mut units = Vec::new();
            collect_units(&root, &mut units)?;
            Ok(units)
        })
        .await
        .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn find_manifest_not_found() {
        let temp_dir = tempdir().unwrap();
        let port = FsUnitPort::new(temp_dir.path());

        let result = port.find_manifest("test", "nonexistent").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn find_manifest_exists() {
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

        let port = FsUnitPort::new(temp_dir.path());

        let result = port.find_manifest("test", "my_unit").await.unwrap();
        let manifest = result.unwrap();
        assert_eq!(manifest.dimensions, vec!["env", "dc"]);
        assert_eq!(manifest.runner_type(), cubtera_model::RunnerType::Terraform);
    }

    #[tokio::test]
    async fn find_manifest_searches_recursively() {
        let temp_dir = tempdir().unwrap();
        let unit_dir = temp_dir.path().join("group").join("nested_unit");
        fs::create_dir_all(&unit_dir).unwrap();
        fs::write(
            unit_dir.join("manifest.toml"),
            "dimensions = []\ntype = \"tf\"",
        )
        .unwrap();

        let port = FsUnitPort::new(temp_dir.path());
        let path = port.get_unit_path("test", "nested_unit").await.unwrap();
        assert!(path.unwrap().ends_with("nested_unit"));
    }

    #[tokio::test]
    async fn list_units_finds_all_manifests() {
        let temp_dir = tempdir().unwrap();

        for name in &["unit1", "unit2"] {
            let unit_dir = temp_dir.path().join(name);
            fs::create_dir_all(&unit_dir).unwrap();
            fs::write(
                unit_dir.join("manifest.toml"),
                "dimensions = []\ntype = \"tf\"",
            )
            .unwrap();
        }

        let port = FsUnitPort::new(temp_dir.path());
        let units = port.list_units("test").await.unwrap();

        assert_eq!(units.len(), 2);
        assert!(units.contains(&"unit1".to_string()));
        assert!(units.contains(&"unit2".to_string()));
    }
}
