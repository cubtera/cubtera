//! File system inventory repository
//!
//! Implements the on-disk inventory naming convention (a stable interface -
//! this is user data, not something we get to redesign):
//!
//! - `{type}/{name}.json` or `{name}{sep}meta.json` -> section "meta"
//! - `{name}{sep}{section}.json` (e.g. `admin:manifest.json`) -> section "{section}"
//! - `.default{sep}{section}.json` -> defaults record (gap-filled by the domain)
//! - `.schema{sep}meta.json` -> JSON-schema for the type, exposed as section "schema"
//! - `{name}{sep}{file}` (non-json) -> include file, copied as `{file}`
//! - `{name}{sep}{folder}/` -> include folder, copied as `{folder}`
//! - names/sections starting with `.` or `#` are reserved/ignored (except `.default`/`.schema`)
//!
//! This adapter performs **no** business logic (no defaults merging, no
//! parent resolution) - it only turns file names into a [`RawDimension`].
//! That keeps a future MongoDB adapter free of duplicated semantics: see
//! [`crate::fs::FsInventoryRepository`] vs. `cubtera_core::services::DimensionService`.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::InventoryRepository;
use cubtera_domain::{IncludeEntry, RawDimension};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tracing::debug;

/// Default separator between a dimension name and its section/include suffix
pub const DEFAULT_SEPARATOR: &str = ":";

/// File system based inventory repository
pub struct FsInventoryRepository {
    base_path: PathBuf,
    separator: String,
}

impl FsInventoryRepository {
    /// Create a new FS inventory repository rooted at `base_path`
    /// (`<inventory_path>`, containing one directory per org).
    pub fn new(base_path: PathBuf) -> Self {
        Self {
            base_path,
            separator: DEFAULT_SEPARATOR.to_string(),
        }
    }

    /// Override the file name separator (default `:`)
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    fn dim_type_dir(&self, org: &str, dim_type: &str) -> PathBuf {
        self.base_path.join(org).join(dim_type)
    }

    /// Read every file/folder that belongs to `record_name` (either a real
    /// dimension name or a reserved name like `.default`/`.schema`) inside
    /// `dir`, and split it into sections + includes per the naming convention.
    fn read_record(&self, dir: &Path, record_name: &str) -> AppResult<Option<RawDimension>> {
        if !dir.exists() {
            return Ok(None);
        }

        let prefix = format!("{record_name}{}", self.separator);
        let mut sections: HashMap<String, Value> = HashMap::new();
        let mut includes: Vec<IncludeEntry> = Vec::new();

        for entry in
            fs::read_dir(dir).map_err(|e| AppError::io(format!("Can't read {dir:?}: {e}")))?
        {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            let is_json = path.is_file() && path.extension().map(|e| e == "json").unwrap_or(false);

            if is_json {
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                let section = if stem == record_name {
                    Some("meta".to_string())
                } else {
                    stem.strip_prefix(&prefix).map(|rest| {
                        if rest.is_empty() {
                            "meta".to_string()
                        } else {
                            rest.to_string()
                        }
                    })
                };

                if let Some(section) = section {
                    let value = read_json_file(&path)?;
                    sections.insert(section, value);
                }
            } else if let Some(rest) = file_name.strip_prefix(&prefix) {
                if !rest.is_empty() {
                    includes.push(IncludeEntry {
                        name: rest.to_string(),
                        source: path.clone(),
                        is_dir: path.is_dir(),
                    });
                }
            }
        }

        if sections.is_empty() && includes.is_empty() {
            return Ok(None);
        }

        Ok(Some(RawDimension {
            name: record_name.to_string(),
            sections,
            includes,
        }))
    }
}

#[async_trait]
impl InventoryRepository for FsInventoryRepository {
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawDimension>> {
        let dir = self.dim_type_dir(org, dim_type);
        debug!("Reading raw dimension {}:{} from {:?}", dim_type, name, dir);
        match self.read_record(&dir, name)? {
            // A dimension only "exists" if it has its own meta record, matching
            // v1 semantics (defaults alone don't create a dimension).
            Some(raw) if raw.sections.contains_key("meta") => Ok(Some(raw)),
            _ => Ok(None),
        }
    }

    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>> {
        let dir = self.dim_type_dir(org, dim_type);
        self.read_record(&dir, ".default")
    }

    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<RawDimension>> {
        let dir = self.dim_type_dir(org, dim_type);
        self.read_record(&dir, ".schema")
    }

    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        let dir = self.dim_type_dir(org, dim_type);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let meta_suffix = format!("{}meta", self.separator);
        let mut names: Vec<String> = Vec::new();

        for entry in fs::read_dir(&dir).map_err(|e| AppError::io(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if !path.is_file() || path.extension().map(|e| e != "json").unwrap_or(true) {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
                continue;
            };
            if stem.starts_with('.') || stem.starts_with('#') {
                continue; // reserved: .default, .schema, # comments/backups
            }
            if stem.contains("schema") {
                continue;
            }
            // Either a bare `{name}.json`, or a `{name}{sep}meta.json` section file.
            if stem.contains(&self.separator) && !stem.ends_with(&meta_suffix) {
                continue;
            }
            let name = stem.trim_end_matches(&meta_suffix).to_string();
            if !name.is_empty() {
                names.push(name);
            }
        }

        names.sort();
        names.dedup();
        Ok(names)
    }

    async fn list_types(&self, org: &str) -> AppResult<Vec<String>> {
        let org_path = self.base_path.join(org);
        if !org_path.exists() {
            return Ok(Vec::new());
        }

        let mut types = Vec::new();
        for entry in fs::read_dir(&org_path).map_err(|e| AppError::io(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    types.push(name.to_string());
                }
            }
        }
        types.sort();
        Ok(types)
    }

    async fn list_orgs(&self) -> AppResult<Vec<String>> {
        if !self.base_path.exists() {
            return Ok(Vec::new());
        }

        let mut orgs = Vec::new();
        for entry in fs::read_dir(&self.base_path).map_err(|e| AppError::io(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    orgs.push(name.to_string());
                }
            }
        }
        orgs.sort();
        Ok(orgs)
    }

    async fn save_raw(&self, org: &str, dim_type: &str, raw: &RawDimension) -> AppResult<()> {
        let dir = self.dim_type_dir(org, dim_type);
        fs::create_dir_all(&dir).map_err(|e| AppError::io(e.to_string()))?;

        for (section, value) in &raw.sections {
            let file_name = format!("{}{}{}.json", raw.name, self.separator, section);
            let content = serde_json::to_string_pretty(value)
                .map_err(|e| AppError::repository(e.to_string()))?;
            fs::write(dir.join(file_name), content).map_err(|e| AppError::io(e.to_string()))?;
        }

        Ok(())
    }

    async fn delete_raw(&self, org: &str, dim_type: &str, name: &str) -> AppResult<()> {
        let dir = self.dim_type_dir(org, dim_type);
        if !dir.exists() {
            return Ok(());
        }

        let bare = format!("{name}.json");
        let prefix = format!("{name}{}", self.separator);

        for entry in fs::read_dir(&dir).map_err(|e| AppError::io(e.to_string()))? {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if file_name == bare || file_name.starts_with(&prefix) {
                if path.is_dir() {
                    fs::remove_dir_all(&path).map_err(|e| AppError::io(e.to_string()))?;
                } else {
                    fs::remove_file(&path).map_err(|e| AppError::io(e.to_string()))?;
                }
            }
        }
        Ok(())
    }
}

fn read_json_file(path: &Path) -> AppResult<Value> {
    let content = fs::read_to_string(path).map_err(|e| AppError::io(format!("{path:?}: {e}")))?;
    serde_json::from_str(&content)
        .map_err(|e| AppError::repository(format!("Invalid JSON in {path:?}: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, content: &str) {
        fs::write(dir.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn test_get_raw_bare_json_maps_to_meta() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&dc_dir).unwrap();
        write(&dc_dir, "prod-use1.json", r#"{"region": "us-east-1"}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let raw = repo
            .get_raw("cubtera", "dc", "prod-use1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw.sections["meta"]["region"], "us-east-1");
    }

    #[tokio::test]
    async fn test_get_raw_named_meta_and_custom_section() {
        let tmp = tempdir().unwrap();
        let svc_dir = tmp.path().join("cubtera").join("service");
        fs::create_dir_all(&svc_dir).unwrap();
        write(&svc_dir, "admin.json", r#"{"cmd": "run"}"#);
        write(&svc_dir, "admin:manifest.json", r#"{"owners": ["team1"]}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let raw = repo
            .get_raw("cubtera", "service", "admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw.sections["meta"]["cmd"], "run");
        assert_eq!(raw.sections["manifest"]["owners"][0], "team1");
    }

    #[tokio::test]
    async fn test_get_raw_missing_meta_returns_none() {
        let tmp = tempdir().unwrap();
        let svc_dir = tmp.path().join("cubtera").join("service");
        fs::create_dir_all(&svc_dir).unwrap();
        // Only a manifest section, no meta -> not a real dimension yet.
        write(&svc_dir, "orphan:manifest.json", r#"{"owners": []}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let raw = repo.get_raw("cubtera", "service", "orphan").await.unwrap();
        assert!(raw.is_none());
    }

    #[tokio::test]
    async fn test_get_raw_defaults() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&dc_dir).unwrap();
        write(&dc_dir, ".default:meta.json", r#"{"region": "us-east-1"}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let defaults = repo
            .get_raw_defaults("cubtera", "dc")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(defaults.sections["meta"]["region"], "us-east-1");
    }

    #[tokio::test]
    async fn test_get_raw_schema() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&dc_dir).unwrap();
        write(
            &dc_dir,
            ".schema:meta.json",
            r#"{"type": "object", "required": ["region"]}"#,
        );

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let schema = repo.get_raw_schema("cubtera", "dc").await.unwrap().unwrap();
        assert_eq!(schema.sections["meta"]["required"][0], "region");
    }

    #[tokio::test]
    async fn test_get_raw_schema_missing_returns_none() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&dc_dir).unwrap();
        write(&dc_dir, "prod.json", r#"{}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        assert!(repo
            .get_raw_schema("cubtera", "dc")
            .await
            .unwrap()
            .is_none());
    }

    #[tokio::test]
    async fn test_list_names_excludes_reserved_and_sections() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&dc_dir).unwrap();
        write(&dc_dir, ".default:meta.json", r#"{}"#);
        write(&dc_dir, ".schema:meta.json", r#"{}"#);
        write(&dc_dir, "prod-use1.json", r#"{}"#);
        write(&dc_dir, "prod-use2:meta.json", r#"{}"#);
        write(&dc_dir, "prod-use2:manifest.json", r#"{}"#);

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let mut names = repo.list_names("cubtera", "dc").await.unwrap();
        names.sort();
        assert_eq!(
            names,
            vec!["prod-use1".to_string(), "prod-use2".to_string()]
        );
    }

    #[tokio::test]
    async fn test_includes_are_collected() {
        let tmp = tempdir().unwrap();
        let unit_dir = tmp.path().join("cubtera").join("dc");
        fs::create_dir_all(&unit_dir).unwrap();
        write(&unit_dir, "prod.json", r#"{}"#);
        write(&unit_dir, "prod:script.sh", "#!/bin/sh\necho hi\n");
        fs::create_dir_all(unit_dir.join("prod:extra")).unwrap();

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let raw = repo
            .get_raw("cubtera", "dc", "prod")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw.includes.len(), 2);
        assert!(raw
            .includes
            .iter()
            .any(|i| i.name == "script.sh" && !i.is_dir));
        assert!(raw.includes.iter().any(|i| i.name == "extra" && i.is_dir));
    }

    #[tokio::test]
    async fn test_list_types_and_orgs() {
        let tmp = tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("cubtera").join("dc")).unwrap();
        fs::create_dir_all(tmp.path().join("cubtera").join("env")).unwrap();
        fs::create_dir_all(tmp.path().join("teracub").join("dc")).unwrap();

        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());
        let orgs = repo.list_orgs().await.unwrap();
        assert_eq!(orgs, vec!["cubtera".to_string(), "teracub".to_string()]);

        let types = repo.list_types("cubtera").await.unwrap();
        assert_eq!(types, vec!["dc".to_string(), "env".to_string()]);
    }

    #[tokio::test]
    async fn test_save_and_delete_raw_roundtrip() {
        let tmp = tempdir().unwrap();
        let repo = FsInventoryRepository::new(tmp.path().to_path_buf());

        let raw = RawDimension::new("prod")
            .with_section("meta", serde_json::json!({"region": "us-east-1"}));
        repo.save_raw("cubtera", "dc", &raw).await.unwrap();

        let loaded = repo
            .get_raw("cubtera", "dc", "prod")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(loaded.sections["meta"]["region"], "us-east-1");

        repo.delete_raw("cubtera", "dc", "prod").await.unwrap();
        assert!(repo
            .get_raw("cubtera", "dc", "prod")
            .await
            .unwrap()
            .is_none());
    }
}
