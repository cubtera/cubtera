//! Deployment log endpoint (wave 2)

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct DlogQuery {
    /// Comma-separated `key:value` filters, e.g. `?q=env:prod,unit:network`
    /// (`serde_urlencoded`, which axum's `Query` extractor uses, can't
    /// collect repeated `q=...&q=...` keys into a `Vec`, so filters share
    /// one param instead). `unit`/`unit_name`, `command`, `exit_code` match
    /// the corresponding entry field exactly; anything else is matched
    /// against the dimensions the run was against.
    q: Option<String>,
    limit: Option<usize>,
}

pub async fn get_logs(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(params): Query<DlogQuery>,
) -> Result<Json<Vec<cubtera_core::ports::DeploymentLogEntry>>, ApiError> {
    let mut query: HashMap<String, String> = HashMap::new();
    for filter in params.q.iter().flat_map(|q| q.split(',')) {
        if let Some((key, value)) = filter.split_once(':') {
            query.insert(key.to_string(), value.to_string());
        }
    }

    let entries = state
        .deployment_log
        .find(&org, &query, params.limit)
        .await?;
    Ok(Json(entries))
}
