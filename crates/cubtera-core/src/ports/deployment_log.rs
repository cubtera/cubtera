//! Deployment log port (interface)
//!
//! Trait for logging deployments.

use crate::error::AppResult;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;

/// A deployment log entry
#[derive(Debug, Clone)]
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

