//! File system (JSONL) deployment log repository
//!
//! One append-only file per org, `{base_path}/{org}.jsonl`, one JSON object
//! per line (newest appended at the end). This is the default deployment
//! log backend - no database required - with `MongoDeploymentLogRepository`
//! (see `crate::mongodb`) as the opt-in alternative for teams that already
//! query dlog data with Mongo tooling.
//!
//! Query/filter logic is shared with the Mongo adapter via
//! `cubtera_core::ports::{entry_matches, matches_all_dimensions}` so the two
//! backends can't drift in what a given `-q key:value` actually matches.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{
    entry_matches, matches_all_dimensions, DeploymentLogEntry, DeploymentLogRepository,
};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;

/// File system (JSONL) deployment log repository
pub struct FsDeploymentLogRepository {
    base_path: PathBuf,
}

impl FsDeploymentLogRepository {
    /// Create a new repository rooted at `base_path` (one `{org}.jsonl` file
    /// per org will be created underneath it on first `save`).
    pub fn new(base_path: PathBuf) -> Self {
        Self { base_path }
    }

    fn log_path(&self, org: &str) -> PathBuf {
        self.base_path.join(format!("{org}.jsonl"))
    }

    fn read_entries(&self, org: &str) -> AppResult<Vec<DeploymentLogEntry>> {
        let path = self.log_path(org);
        if !path.exists() {
            return Ok(Vec::new());
        }

        let file =
            std::fs::File::open(&path).map_err(|e| AppError::io(format!("{path:?}: {e}")))?;
        let mut entries = Vec::new();
        for line in BufReader::new(file).lines() {
            let line = line.map_err(|e| AppError::io(e.to_string()))?;
            if line.trim().is_empty() {
                continue;
            }
            let entry: DeploymentLogEntry = serde_json::from_str(&line).map_err(|e| {
                AppError::repository(format!("Invalid dlog entry in {path:?}: {e}"))
            })?;
            entries.push(entry);
        }
        Ok(entries)
    }

    /// Entries newest-first, most recent `limit` (all of them if `None`).
    fn newest_first(
        mut entries: Vec<DeploymentLogEntry>,
        limit: Option<usize>,
    ) -> Vec<DeploymentLogEntry> {
        entries.sort_by_key(|e| std::cmp::Reverse(e.timestamp));
        if let Some(limit) = limit {
            entries.truncate(limit);
        }
        entries
    }
}

#[async_trait]
impl DeploymentLogRepository for FsDeploymentLogRepository {
    async fn save(&self, entry: &DeploymentLogEntry) -> AppResult<()> {
        std::fs::create_dir_all(&self.base_path).map_err(|e| AppError::io(e.to_string()))?;
        let path = self.log_path(&entry.org);
        let line = serde_json::to_string(entry)
            .map_err(|e| AppError::repository(format!("Failed to encode dlog entry: {e}")))?;

        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| AppError::io(format!("{path:?}: {e}")))?;
        writeln!(file, "{line}").map_err(|e| AppError::io(e.to_string()))?;
        Ok(())
    }

    async fn find(
        &self,
        org: &str,
        query: &HashMap<String, String>,
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries = self
            .read_entries(org)?
            .into_iter()
            .filter(|entry| entry_matches(entry, query))
            .collect();
        Ok(Self::newest_first(entries, limit))
    }

    async fn find_by_dimensions(
        &self,
        org: &str,
        dimensions: &[String],
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries = self
            .read_entries(org)?
            .into_iter()
            .filter(|entry| matches_all_dimensions(entry, dimensions))
            .collect();
        Ok(Self::newest_first(entries, limit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;

    fn entry(
        unit_name: &str,
        org: &str,
        dimensions: &[&str],
        timestamp: i64,
    ) -> DeploymentLogEntry {
        DeploymentLogEntry {
            unit_name: unit_name.to_string(),
            org: org.to_string(),
            dimensions: dimensions.iter().map(|s| s.to_string()).collect(),
            command: "apply".to_string(),
            exit_code: 0,
            timestamp,
            duration_ms: 0,
            git_shas: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    fn repo(dir: &Path) -> FsDeploymentLogRepository {
        FsDeploymentLogRepository::new(dir.to_path_buf())
    }

    #[tokio::test]
    async fn save_appends_one_line_per_entry() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());

        repo.save(&entry("network", "cubtera", &["env:prod"], 1))
            .await
            .unwrap();
        repo.save(&entry("network", "cubtera", &["env:staging"], 2))
            .await
            .unwrap();

        let contents = std::fs::read_to_string(tmp.path().join("cubtera.jsonl")).unwrap();
        assert_eq!(contents.lines().count(), 2);
    }

    #[tokio::test]
    async fn find_filters_by_query_and_sorts_newest_first() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        repo.save(&entry("network", "cubtera", &["env:prod"], 1))
            .await
            .unwrap();
        repo.save(&entry("app", "cubtera", &["env:prod"], 3))
            .await
            .unwrap();
        repo.save(&entry("network", "cubtera", &["env:staging"], 2))
            .await
            .unwrap();

        let mut query = HashMap::new();
        query.insert("env".to_string(), "prod".to_string());
        let results = repo.find("cubtera", &query, None).await.unwrap();

        assert_eq!(results.len(), 2);
        assert_eq!(results[0].unit_name, "app"); // timestamp 3, newest first
        assert_eq!(results[1].unit_name, "network"); // timestamp 1
    }

    #[tokio::test]
    async fn find_respects_limit() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        for i in 0..5 {
            repo.save(&entry("network", "cubtera", &["env:prod"], i))
                .await
                .unwrap();
        }

        let mut query = HashMap::new();
        query.insert("env".to_string(), "prod".to_string());
        let results = repo.find("cubtera", &query, Some(2)).await.unwrap();
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    async fn find_by_dimensions_requires_every_dimension() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        repo.save(&entry("network", "cubtera", &["env:prod", "dc:use1"], 1))
            .await
            .unwrap();
        repo.save(&entry("network", "cubtera", &["env:prod"], 2))
            .await
            .unwrap();

        let results = repo
            .find_by_dimensions(
                "cubtera",
                &["env:prod".to_string(), "dc:use1".to_string()],
                None,
            )
            .await
            .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].timestamp, 1);
    }

    #[tokio::test]
    async fn entries_are_scoped_to_their_own_org_file() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        repo.save(&entry("network", "cubtera", &["env:prod"], 1))
            .await
            .unwrap();
        repo.save(&entry("network", "teracub", &["env:prod"], 2))
            .await
            .unwrap();

        let query = HashMap::new();
        let cubtera_results = repo.find("cubtera", &query, None).await.unwrap();
        let teracub_results = repo.find("teracub", &query, None).await.unwrap();

        assert_eq!(cubtera_results.len(), 1);
        assert_eq!(teracub_results.len(), 1);
    }

    #[tokio::test]
    async fn find_on_missing_file_returns_empty() {
        let tmp = tempdir().unwrap();
        let repo = repo(tmp.path());
        let query = HashMap::new();
        let results = repo.find("cubtera", &query, None).await.unwrap();
        assert!(results.is_empty());
    }
}
