//! Deployment log query endpoint - v3-native, reads
//! `cubtera_store::LegacyDeploymentLogRow` (the same SQLite table v2's
//! `DeploymentLogRepository`/`cubtera run` write to) directly, no
//! `cubtera-core`/`cubtera-persistence` dependency. Same seam rationale as
//! `cubtera log get` (`crates/cubtera/src/commands/log.rs`).

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use cubtera_store::SqliteStore;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct DlogQuery {
    /// Comma-separated `key:value` filters, e.g. `?q=unit:tf_unit02,env:prod`
    q: Option<String>,
    limit: Option<usize>,
}

pub async fn get_deployment_log(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(params): Query<DlogQuery>,
) -> Result<Json<Value>, ApiError> {
    let store = SqliteStore::open(&state.config.store_path)
        .map_err(|e| ApiError::bad_request(format!("failed to open store: {e}")))?;

    let mut query = HashMap::new();
    for pair in params.q.iter().flat_map(|q| q.split(',')) {
        if let Some((k, v)) = pair.split_once(':') {
            query.insert(k.to_string(), v.to_string());
        }
    }

    let mut rows = store
        .find_legacy_deployment_log(&org)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    rows.retain(|row| row.matches(&query));
    rows.sort_by_key(|row| std::cmp::Reverse(row.timestamp));
    rows.truncate(params.limit.unwrap_or(10));

    Ok(Json(json!(rows)))
}
