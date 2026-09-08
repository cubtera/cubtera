//! Deployment log query endpoint - `cubtera-api`'s `dlog.rs`, same seam
//! rationale as `inventory.rs`.

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use cubtera_persistence::Repositories;
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
    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;

    let mut query = HashMap::new();
    for pair in params.q.iter().flat_map(|q| q.split(',')) {
        if let Some((k, v)) = pair.split_once(':') {
            query.insert(k.to_string(), v.to_string());
        }
    }

    let entries = repos
        .deployment_log
        .find(&org, &query, params.limit.or(Some(10)))
        .await?;
    Ok(Json(json!(entries)))
}
