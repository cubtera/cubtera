//! File system (JSON) unit state repository
//!
//! One `outputs.json` file per published key, laid out as
//! `{base_path}/{org}/{unit}/{dims joined by "/"}/{ext joined by "/"}/outputs.json`
//! (an empty `dims`/`ext` uses a literal `_root` directory segment) - the
//! default backend, with `MongoUnitStateRepository` (see `crate::mongodb`)
//! as the opt-in alternative for teams that already run Mongo for the
//! deployment log.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::UnitStateRepository;
use cubtera_domain::{UnitStateKey, UnitStateRecord};
use std::path::{Path, PathBuf};

/// File system (JSON) unit state repository
pub struct FsUnitStateRepository {
    base_path: PathBuf,
}

impl FsUnitStateRepository {
    /// Create a new repository rooted at `base_path`.
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    /// `{base_path}/{org}/{unit}/{dims...}/{ext...}/outputs.json`. `dims`
    /// and `ext` entries are `type:name` strings, valid as path segments on
    /// every platform this project targets (unix-first, per `AGENTS.md`).
    fn record_path(&self, key: &UnitStateKey) -> PathBuf {
        let mut path = self.base_path.join(&key.org).join(&key.unit);
        if key.dims.is_empty() {
            path = path.join("_root");
        } else {
            for dim in &key.dims {
                path = path.join(dim);
            }
        }
        for ext in &key.ext {
            path = path.join(ext);
        }
        path.join("outputs.json")
    }

    /// Walk every `outputs.json` under `{base_path}/{org}/{unit}`.
    fn list_paths(base: &Path) -> AppResult<Vec<PathBuf>> {
        let mut found = Vec::new();
        if !base.exists() {
            return Ok(found);
        }
        Self::walk(base, &mut found)?;
        Ok(found)
    }

    fn walk(dir: &Path, found: &mut Vec<PathBuf>) -> AppResult<()> {
        for entry in std::fs::read_dir(dir).map_err(|e| AppError::io(format!("{dir:?}: {e}")))? {
            let entry = entry.map_err(|e| AppError::io(e.to_string()))?;
            let path = entry.path();
            if path.is_dir() {
                Self::walk(&path, found)?;
            } else if path
                .file_name()
                .map(|n| n == "outputs.json")
                .unwrap_or(false)
            {
                found.push(path);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl UnitStateRepository for FsUnitStateRepository {
    async fn get(&self, key: &UnitStateKey) -> AppResult<Option<UnitStateRecord>> {
        let path = self.record_path(key);
        match tokio::fs::read_to_string(&path).await {
            Ok(content) => {
                let record: UnitStateRecord = serde_json::from_str(&content).map_err(|e| {
                    AppError::repository(format!("invalid unit state record in {path:?}: {e}"))
                })?;
                Ok(Some(record))
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::io(format!("{path:?}: {e}"))),
        }
    }

    async fn put(&self, record: &UnitStateRecord) -> AppResult<()> {
        let path = self.record_path(&record.key());
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| AppError::io(format!("failed to create {parent:?}: {e}")))?;
        }
        let content = serde_json::to_string_pretty(record)
            .map_err(|e| AppError::repository(format!("failed to encode unit state: {e}")))?;
        tokio::fs::write(&path, content)
            .await
            .map_err(|e| AppError::io(format!("failed to write {path:?}: {e}")))
    }

    async fn delete(&self, key: &UnitStateKey) -> AppResult<()> {
        let path = self.record_path(key);
        match tokio::fs::remove_file(&path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(AppError::io(format!("failed to remove {path:?}: {e}"))),
        }
    }

    async fn list(&self, org: &str, unit: &str) -> AppResult<Vec<UnitStateRecord>> {
        let base = self.base_path.join(org).join(unit);
        let paths = Self::list_paths(&base)?;
        let mut records = Vec::with_capacity(paths.len());
        for path in paths {
            let content = tokio::fs::read_to_string(&path)
                .await
                .map_err(|e| AppError::io(format!("{path:?}: {e}")))?;
            let record: UnitStateRecord = serde_json::from_str(&content).map_err(|e| {
                AppError::repository(format!("invalid unit state record in {path:?}: {e}"))
            })?;
            records.push(record);
        }
        Ok(records)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn record(unit: &str, dims: &[&str], outputs: serde_json::Value) -> UnitStateRecord {
        UnitStateRecord {
            org: "cubtera".to_string(),
            unit: unit.to_string(),
            dims: dims.iter().map(|s| s.to_string()).collect(),
            ext: vec![],
            outputs,
            updated_at: 42,
        }
    }

    fn repo(dir: &Path) -> FsUnitStateRepository {
        FsUnitStateRepository::new(dir.to_path_buf())
    }

    #[tokio::test]
    async fn put_then_get_round_trips() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let record = record(
            "network",
            &["dome:prod"],
            serde_json::json!({"vpc_id": "vpc-1"}),
        );

        repo.put(&record).await.unwrap();
        let fetched = repo.get(&record.key()).await.unwrap().unwrap();
        assert_eq!(fetched.outputs, serde_json::json!({"vpc_id": "vpc-1"}));
    }

    #[tokio::test]
    async fn get_missing_key_returns_none() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let key = UnitStateKey::new("cubtera", "network", vec!["dome:prod".to_string()], vec![]);
        assert!(repo.get(&key).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn put_overwrites_existing_record_for_same_key() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let first = record(
            "network",
            &["dome:prod"],
            serde_json::json!({"vpc_id": "vpc-1"}),
        );
        let second = record(
            "network",
            &["dome:prod"],
            serde_json::json!({"vpc_id": "vpc-2"}),
        );

        repo.put(&first).await.unwrap();
        repo.put(&second).await.unwrap();

        let fetched = repo.get(&first.key()).await.unwrap().unwrap();
        assert_eq!(fetched.outputs, serde_json::json!({"vpc_id": "vpc-2"}));
    }

    #[tokio::test]
    async fn delete_removes_record() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let record = record("network", &["dome:prod"], serde_json::json!({}));
        repo.put(&record).await.unwrap();

        repo.delete(&record.key()).await.unwrap();
        assert!(repo.get(&record.key()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn delete_on_missing_key_is_noop() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let key = UnitStateKey::new("cubtera", "network", vec!["dome:prod".to_string()], vec![]);
        repo.delete(&key).await.unwrap();
    }

    #[tokio::test]
    async fn list_returns_every_record_for_unit_across_dims() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        repo.put(&record(
            "network",
            &["dome:prod"],
            serde_json::json!({"a": 1}),
        ))
        .await
        .unwrap();
        repo.put(&record(
            "network",
            &["dome:staging"],
            serde_json::json!({"a": 2}),
        ))
        .await
        .unwrap();
        repo.put(&record(
            "other_unit",
            &["dome:prod"],
            serde_json::json!({"a": 3}),
        ))
        .await
        .unwrap();

        let records = repo.list("cubtera", "network").await.unwrap();
        assert_eq!(records.len(), 2);
    }

    #[tokio::test]
    async fn list_on_unpublished_unit_returns_empty() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        assert!(repo.list("cubtera", "network").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn key_with_no_dims_uses_root_placeholder() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let record = record("network", &[], serde_json::json!({"a": 1}));

        repo.put(&record).await.unwrap();
        assert!(tmp
            .path()
            .join("cubtera/network/_root/outputs.json")
            .exists());
    }

    #[tokio::test]
    async fn keys_are_scoped_to_their_own_org() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let mut cubtera_record = record("network", &["dome:prod"], serde_json::json!({"a": 1}));
        cubtera_record.org = "cubtera".to_string();
        let mut teracub_record = record("network", &["dome:prod"], serde_json::json!({"a": 2}));
        teracub_record.org = "teracub".to_string();

        repo.put(&cubtera_record).await.unwrap();
        repo.put(&teracub_record).await.unwrap();

        let cubtera_fetched = repo.get(&cubtera_record.key()).await.unwrap().unwrap();
        let teracub_fetched = repo.get(&teracub_record.key()).await.unwrap().unwrap();
        assert_eq!(cubtera_fetched.outputs, serde_json::json!({"a": 1}));
        assert_eq!(teracub_fetched.outputs, serde_json::json!({"a": 2}));
    }
}
