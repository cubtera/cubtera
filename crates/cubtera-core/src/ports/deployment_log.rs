//! Deployment log port (interface)
//!
//! Trait for logging deployments.

use crate::error::AppResult;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// A deployment log entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentLogEntry {
    /// Unit name
    pub unit_name: String,
    /// Organization
    pub org: String,
    /// Dimension keys (e.g., ["env:prod", "dc:us-east-1"])
    pub dimensions: Vec<String>,
    /// Command executed
    pub command: String,
    /// Exit code
    pub exit_code: i32,
    /// Timestamp (Unix epoch seconds)
    pub timestamp: i64,
    /// Duration in milliseconds
    pub duration_ms: u64,
    /// Git SHAs
    pub git_shas: HashMap<String, String>,
    /// Additional metadata (from runner context)
    pub metadata: HashMap<String, Value>,
}

/// Repository for deployment logs
#[async_trait]
pub trait DeploymentLogRepository: Send + Sync {
    /// Save a deployment log entry
    async fn save(&self, entry: &DeploymentLogEntry) -> AppResult<()>;

    /// Find deployment logs by query
    async fn find(
        &self,
        org: &str,
        query: &HashMap<String, String>,
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>>;

    /// Find deployment logs by dimension keys
    async fn find_by_dimensions(
        &self,
        org: &str,
        dimensions: &[String],
        limit: Option<usize>,
    ) -> AppResult<Vec<DeploymentLogEntry>>;
}

/// Whether `entry` satisfies every `key:value` pair in `query`. `unit`/
/// `unit_name` and `command` match the corresponding top-level fields
/// (exact string match); everything else is treated as a dimension type and
/// checked against `entry.dimensions` (`type:name` strings) - so `-q
/// env:prod` on the CLI filters to entries run against `env:prod` (or any
/// dimension chain that included it).
///
/// Shared by every [`DeploymentLogRepository`] adapter so query semantics
/// don't drift between backends (fs-jsonl filters in-process; a Mongo
/// adapter can use this for the same in-process fallback, or translate
/// simple cases to a native query and keep this as the ground truth for
/// tests).
pub fn entry_matches(entry: &DeploymentLogEntry, query: &HashMap<String, String>) -> bool {
    query.iter().all(|(key, value)| match key.as_str() {
        "unit" | "unit_name" => entry.unit_name == *value,
        "command" => entry.command == *value,
        "exit_code" => entry.exit_code.to_string() == *value,
        _ => entry.dimensions.contains(&format!("{key}:{value}")),
    })
}

/// Whether `entry` was run against every dimension key in `dimensions`
/// (`type:name` strings).
pub fn matches_all_dimensions(entry: &DeploymentLogEntry, dimensions: &[String]) -> bool {
    dimensions.iter().all(|d| entry.dimensions.contains(d))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> DeploymentLogEntry {
        DeploymentLogEntry {
            unit_name: "network".to_string(),
            org: "cubtera".to_string(),
            dimensions: vec!["dome:prod".to_string(), "env:prod".to_string()],
            command: "apply".to_string(),
            exit_code: 0,
            timestamp: 1_700_000_000,
            duration_ms: 1234,
            git_shas: HashMap::new(),
            metadata: HashMap::new(),
        }
    }

    #[test]
    fn entry_matches_by_unit_name() {
        let mut query = HashMap::new();
        query.insert("unit".to_string(), "network".to_string());
        assert!(entry_matches(&entry(), &query));

        query.insert("unit".to_string(), "other".to_string());
        assert!(!entry_matches(&entry(), &query));
    }

    #[test]
    fn entry_matches_by_dimension() {
        let mut query = HashMap::new();
        query.insert("env".to_string(), "prod".to_string());
        assert!(entry_matches(&entry(), &query));

        query.insert("env".to_string(), "staging".to_string());
        assert!(!entry_matches(&entry(), &query));
    }

    #[test]
    fn entry_matches_requires_every_query_key() {
        let mut query = HashMap::new();
        query.insert("unit".to_string(), "network".to_string());
        query.insert("command".to_string(), "destroy".to_string());
        assert!(!entry_matches(&entry(), &query));
    }

    #[test]
    fn matches_all_dimensions_requires_every_dimension() {
        assert!(matches_all_dimensions(&entry(), &["env:prod".to_string()]));
        assert!(!matches_all_dimensions(
            &entry(),
            &["env:prod".to_string(), "dc:us-east-1".to_string()]
        ));
    }
}
