//! `FsInventoryPort`: a from-scratch, v3-native port of v2's
//! `cubtera_persistence::fs::FsInventoryRepository`, implementing
//! `cubtera_app::ports::InventoryPort` directly - no dependency on
//! `cubtera-core`/`cubtera-persistence`.
//!
//! Implements the on-disk inventory naming convention (a stable interface -
//! this is user data, not something we get to redesign; see
//! `AGENTS.md`'s "Inventory on-disk format" section):
//!
//! - `{type}/{name}.json` or `{name}{sep}meta.json` -> section "meta"
//! - `{name}{sep}{section}.json` (e.g. `admin:manifest.json`) -> section "{section}"
//! - `.default{sep}{section}.json` -> defaults record (gap-filled by `cubtera-app`)
//! - `.schema{sep}meta.json` -> JSON-schema for the type
//! - `{name}{sep}{file}` (non-json) -> include file, copied as `{file}`
//! - `{name}{sep}{folder}/` -> include folder, copied as `{folder}`
//! - names/sections starting with `.` or `#` are reserved/ignored (except `.default`/`.schema`)
//!
//! Every blocking `std::fs` call runs inside `tokio::task::spawn_blocking`,
//! per the workspace's async-discipline rule - directory reads here are a
//! handful of small files, not worth a full `tokio::fs` rewrite of the
//! (already-tested-elsewhere) traversal logic.

use async_trait::async_trait;
use cubtera_app::ports::{InventoryPort, RawSections};
use cubtera_app::{AppError, AppResult};
use cubtera_model::IncludeEntry;
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Default separator between a dimension name and its section/include suffix.
pub const DEFAULT_SEPARATOR: &str = ":";

/// A raw dimension record as read off disk: JSON sections plus non-JSON includes.
struct RawRecord {
    sections: HashMap<String, Value>,
    includes: Vec<IncludeEntry>,
}

/// FS-backed `InventoryPort`, rooted at `<inventory_path>` (one directory per org).
pub struct FsInventoryPort {
    base_path: PathBuf,
    separator: String,
}

impl FsInventoryPort {
    /// Create a new FS inventory port rooted at `base_path`.
    pub fn new(base_path: impl Into<PathBuf>) -> Self {
        Self {
            base_path: base_path.into(),
            separator: DEFAULT_SEPARATOR.to_string(),
        }
    }

    /// Override the file name separator (default `:`).
    pub fn with_separator(mut self, separator: impl Into<String>) -> Self {
        self.separator = separator.into();
        self
    }

    fn dim_type_dir(&self, org: &str, dim_type: &str) -> PathBuf {
        self.base_path.join(org).join(dim_type)
    }

    async fn read_record(
        &self,
        dir: PathBuf,
        record_name: String,
        separator: String,
    ) -> AppResult<Option<RawRecord>> {
        tokio::task::spawn_blocking(move || read_record_blocking(&dir, &record_name, &separator))
            .await
            .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }
}

fn read_record_blocking(
    dir: &Path,
    record_name: &str,
    separator: &str,
) -> AppResult<Option<RawRecord>> {
    if !dir.exists() {
        return Ok(None);
    }

    let prefix = format!("{record_name}{separator}");
    let mut sections: HashMap<String, Value> = HashMap::new();
    let mut includes: Vec<IncludeEntry> = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| AppError::backend(format!("{dir:?}: {e}")))? {
        let entry = entry.map_err(|e| AppError::backend(e.to_string()))?;
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

    Ok(Some(RawRecord { sections, includes }))
}

fn read_json_file(path: &Path) -> AppResult<Value> {
    let content =
        fs::read_to_string(path).map_err(|e| AppError::backend(format!("{path:?}: {e}")))?;
    serde_json::from_str(&content)
        .map_err(|e| AppError::backend(format!("invalid JSON in {path:?}: {e}")))
}

#[async_trait]
impl InventoryPort for FsInventoryPort {
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawSections>> {
        let dir = self.dim_type_dir(org, dim_type);
        match self
            .read_record(dir, name.to_string(), self.separator.clone())
            .await?
        {
            // A dimension only "exists" if it has its own meta record,
            // matching v1 semantics (defaults alone don't create a dimension).
            Some(raw) if raw.sections.contains_key("meta") => {
                Ok(Some(raw.sections.into_iter().collect()))
            }
            _ => Ok(None),
        }
    }

    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawSections>> {
        let dir = self.dim_type_dir(org, dim_type);
        let raw = self
            .read_record(dir, ".default".to_string(), self.separator.clone())
            .await?;
        Ok(raw.map(|r| r.sections.into_iter().collect()))
    }

    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<Value>> {
        let dir = self.dim_type_dir(org, dim_type);
        let raw = self
            .read_record(dir, ".schema".to_string(), self.separator.clone())
            .await?;
        Ok(raw.and_then(|r| r.sections.get("meta").cloned()))
    }

    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>> {
        let dir = self.dim_type_dir(org, dim_type);
        let separator = self.separator.clone();
        tokio::task::spawn_blocking(move || list_names_blocking(&dir, &separator))
            .await
            .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }

    async fn list_includes(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Vec<IncludeEntry>> {
        let dir = self.dim_type_dir(org, dim_type);
        let raw = self
            .read_record(dir, name.to_string(), self.separator.clone())
            .await?;
        Ok(raw.map(|r| r.includes).unwrap_or_default())
    }

    async fn list_default_includes(
        &self,
        org: &str,
        dim_type: &str,
    ) -> AppResult<Vec<IncludeEntry>> {
        let dir = self.dim_type_dir(org, dim_type);
        let raw = self
            .read_record(dir, ".default".to_string(), self.separator.clone())
            .await?;
        Ok(raw.map(|r| r.includes).unwrap_or_default())
    }

    async fn list_types(&self, org: &str) -> AppResult<Vec<String>> {
        let dir = self.base_path.join(org);
        tokio::task::spawn_blocking(move || list_subdirs_blocking(&dir))
            .await
            .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }

    async fn list_orgs(&self) -> AppResult<Vec<String>> {
        let dir = self.base_path.clone();
        tokio::task::spawn_blocking(move || list_subdirs_blocking(&dir))
            .await
            .map_err(|e| AppError::backend(format!("blocking task panicked: {e}")))?
    }
}

/// Every immediate subdirectory name of `dir`, sorted - the on-disk
/// convention for "every org" (`<inventory_path>/{org}`) and "every
/// dimension type" (`<inventory_path>/{org}/{dim_type}`) alike. Ported
/// verbatim from v2's `FsInventoryRepository::list_types`/`list_orgs`.
fn list_subdirs_blocking(dir: &Path) -> AppResult<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(dir).map_err(|e| AppError::backend(format!("{dir:?}: {e}")))? {
        let entry = entry.map_err(|e| AppError::backend(e.to_string()))?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    Ok(names)
}

fn list_names_blocking(dir: &Path, separator: &str) -> AppResult<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let meta_suffix = format!("{separator}meta");
    let mut names: Vec<String> = Vec::new();

    for entry in fs::read_dir(dir).map_err(|e| AppError::backend(e.to_string()))? {
        let entry = entry.map_err(|e| AppError::backend(e.to_string()))?;
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
        if stem.contains(separator) && !stem.ends_with(&meta_suffix) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn write(dir: &Path, name: &str, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(name), content).unwrap();
    }

    #[tokio::test]
    async fn get_raw_bare_json_maps_to_meta() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        write(&dc_dir, "prod-use1.json", r#"{"region": "us-east-1"}"#);

        let port = FsInventoryPort::new(tmp.path());
        let raw = port
            .get_raw("cubtera", "dc", "prod-use1")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw["meta"]["region"], "us-east-1");
    }

    #[tokio::test]
    async fn get_raw_named_meta_and_custom_section() {
        let tmp = tempdir().unwrap();
        let svc_dir = tmp.path().join("cubtera").join("service");
        write(&svc_dir, "admin.json", r#"{"cmd": "run"}"#);
        write(&svc_dir, "admin:manifest.json", r#"{"owners": ["team1"]}"#);

        let port = FsInventoryPort::new(tmp.path());
        let raw = port
            .get_raw("cubtera", "service", "admin")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(raw["meta"]["cmd"], "run");
        assert_eq!(raw["manifest"]["owners"][0], "team1");
    }

    #[tokio::test]
    async fn get_raw_missing_meta_returns_none() {
        let tmp = tempdir().unwrap();
        let svc_dir = tmp.path().join("cubtera").join("service");
        // Only a manifest section, no meta -> not a real dimension yet.
        write(&svc_dir, "orphan:manifest.json", r#"{"owners": []}"#);

        let port = FsInventoryPort::new(tmp.path());
        let raw = port.get_raw("cubtera", "service", "orphan").await.unwrap();
        assert!(raw.is_none());
    }

    #[tokio::test]
    async fn get_raw_defaults_and_schema() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        write(&dc_dir, ".default:meta.json", r#"{"region": "us-east-1"}"#);
        write(
            &dc_dir,
            ".schema:meta.json",
            r#"{"type": "object", "required": ["region"]}"#,
        );

        let port = FsInventoryPort::new(tmp.path());
        let defaults = port
            .get_raw_defaults("cubtera", "dc")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(defaults["meta"]["region"], "us-east-1");

        let schema = port.get_raw_schema("cubtera", "dc").await.unwrap().unwrap();
        assert_eq!(schema["required"][0], "region");
    }

    #[tokio::test]
    async fn list_names_excludes_reserved_and_sections() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        write(&dc_dir, ".default:meta.json", r#"{}"#);
        write(&dc_dir, ".schema:meta.json", r#"{}"#);
        write(&dc_dir, "prod-use1.json", r#"{}"#);
        write(&dc_dir, "prod-use2:meta.json", r#"{}"#);
        write(&dc_dir, "prod-use2:manifest.json", r#"{}"#);

        let port = FsInventoryPort::new(tmp.path());
        let mut names = port.list_names("cubtera", "dc").await.unwrap();
        names.sort();
        assert_eq!(
            names,
            vec!["prod-use1".to_string(), "prod-use2".to_string()]
        );
    }

    #[tokio::test]
    async fn list_includes_collects_files_and_dirs() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        write(&dc_dir, "prod.json", r#"{}"#);
        write(&dc_dir, "prod:script.sh", "#!/bin/sh\necho hi\n");
        fs::create_dir_all(dc_dir.join("prod:extra")).unwrap();

        let port = FsInventoryPort::new(tmp.path());
        let includes = port.list_includes("cubtera", "dc", "prod").await.unwrap();
        assert_eq!(includes.len(), 2);
        assert!(includes.iter().any(|i| i.name == "script.sh" && !i.is_dir));
        assert!(includes.iter().any(|i| i.name == "extra" && i.is_dir));
    }

    #[tokio::test]
    async fn list_default_includes_collects_default_files() {
        let tmp = tempdir().unwrap();
        let dc_dir = tmp.path().join("cubtera").join("dc");
        write(&dc_dir, ".default:meta.json", r#"{}"#);
        write(&dc_dir, ".default:keys.pem", "cert-bytes");

        let port = FsInventoryPort::new(tmp.path());
        let includes = port.list_default_includes("cubtera", "dc").await.unwrap();
        assert_eq!(includes.len(), 1);
        assert_eq!(includes[0].name, "keys.pem");
    }

    #[tokio::test]
    async fn list_types_and_orgs_scan_subdirectories() {
        let tmp = tempdir().unwrap();
        write(&tmp.path().join("cubtera").join("dc"), "prod.json", "{}");
        write(&tmp.path().join("cubtera").join("env"), "prod.json", "{}");
        write(&tmp.path().join("other-org").join("dc"), "prod.json", "{}");

        let port = FsInventoryPort::new(tmp.path());
        let mut types = port.list_types("cubtera").await.unwrap();
        types.sort();
        assert_eq!(types, vec!["dc".to_string(), "env".to_string()]);

        let mut orgs = port.list_orgs().await.unwrap();
        orgs.sort();
        assert_eq!(orgs, vec!["cubtera".to_string(), "other-org".to_string()]);
    }

    #[tokio::test]
    async fn list_types_and_orgs_on_missing_root_return_empty() {
        let tmp = tempdir().unwrap();
        let port = FsInventoryPort::new(tmp.path().join("nope"));
        assert!(port.list_types("cubtera").await.unwrap().is_empty());
        assert!(port.list_orgs().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn missing_org_or_type_returns_empty_not_error() {
        let tmp = tempdir().unwrap();
        let port = FsInventoryPort::new(tmp.path());
        assert!(port.get_raw("nope", "dc", "prod").await.unwrap().is_none());
        assert!(port.list_names("nope", "dc").await.unwrap().is_empty());
        assert!(port
            .list_includes("nope", "dc", "prod")
            .await
            .unwrap()
            .is_empty());
    }
}
