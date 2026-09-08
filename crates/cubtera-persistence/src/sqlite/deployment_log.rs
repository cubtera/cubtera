//! SQLite-backed [`DeploymentLogRepository`], via `cubtera-store`'s
//! [`SqliteStore`] legacy seam (see `cubtera_store::legacy` and
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9). Replaces v2's
//! FS-jsonl/Mongo choice - the deployment log is now always backed by the
//! same SQLite `Store` file as unit state.
//!
//! Query semantics (`entry_matches`/`matches_all_dimensions`) are shared
//! with the FS adapter this replaces (they're free functions in
//! `cubtera_core::ports`), so the same contract suite
//! (`tests/support_dlog/mod.rs`) passes unchanged against this backend -
//! see `tests/deployment_log_contract_sqlite.rs`.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{
    entry_matches, matches_all_dimensions, DeploymentLogEntry, DeploymentLogRepository,
};
use cubtera_store::{LegacyDeploymentLogRow, SqliteStore, StoreError};
use std::collections::HashMap;
use std::sync::Arc;

pub struct SqliteDeploymentLogRepository {
    store: Arc<SqliteStore>,
}

impl SqliteDeploymentLogRepository {
    pub fn new(store: Arc<SqliteStore>) -> Self {
        Self { store }
    }
}

fn to_app_error(e: StoreError) -> AppError {
    AppError::repository(format!("SQLite store error: {e}"))
}

fn to_row(entry: &DeploymentLogEntry) -> LegacyDeploymentLogRow {
    LegacyDeploymentLogRow {
        org: entry.org.clone(),
        unit_name: entry.unit_name.clone(),
        dimensions: entry.dimensions.clone(),
        command: entry.command.clone(),
        exit_code: entry.exit_code,
        timestamp: entry.timestamp,
        duration_ms: entry.duration_ms,
        git_shas: entry.git_shas.clone().into_iter().collect(),
        metadata: entry.metadata.clone().into_iter().collect(),
    }
}

fn from_row(row: LegacyDeploymentLogRow) -> DeploymentLogEntry {
    DeploymentLogEntry {
        unit_name: row.unit_name,
        org: row.org,
        dimensions: row.dimensions,
        command: row.command,
        exit_code: row.exit_code,
        timestamp: row.timestamp,
        duration_ms: row.duration_ms,
        git_shas: row.git_shas.into_iter().collect(),
        metadata: row.metadata.into_iter().collect(),
    }
}

/// Entries newest-first, most recent `limit` (all of them if `None`) - the
/// exact ordering/truncation the FS adapter applied.
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

#[async_trait]
impl DeploymentLogRepository for SqliteDeploymentLogRepository {
    async fn save(&self, entry: &DeploymentLogEntry) -> AppResult<()> {
        self.store
            .append_legacy_deployment_log(to_row(entry))
            .await
            .map_err(to_app_error)
    }

    async fn find(
        &self,
        org: &str,
        query: &HashMap<String, String>,
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries: Vec<DeploymentLogEntry> = self
            .store
            .find_legacy_deployment_log(org)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(from_row)
            .filter(|entry| entry_matches(entry, query))
            .collect();
        Ok(newest_first(entries, limit))
    }

    async fn find_by_dimensions(
        &self,
        org: &str,
        dimensions: &[String],
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>> {
        let entries: Vec<DeploymentLogEntry> = self
            .store
            .find_legacy_deployment_log(org)
            .await
            .map_err(to_app_error)?
            .into_iter()
            .map(from_row)
            .filter(|entry| matches_all_dimensions(entry, dimensions))
            .collect();
        Ok(newest_first(entries, limit))
    }
}
