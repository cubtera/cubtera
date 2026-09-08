//! Unit state (cross-unit `[outputs]`) read endpoint
//!
//! A direct, read-only lookup of whatever a producer unit last published via
//! `[outputs] publish = true` - not a consumer's `[inputs]` projection
//! (`cubtera_domain::project_state_key`), which only runs inside
//! `cubtera run`. The caller must supply the exact `dims`/`ext` the producer
//! ran with, mirroring `cubtera state get`.

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use cubtera_core::error::AppError;
use cubtera_domain::{UnitStateKey, UnitStateRecord};
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub struct UnitStateQuery {
    /// Comma-separated `type:name` dimensions the producer ran with, e.g.
    /// `?dims=dome:prod` or `?dims=env:prod,dc:use1`
    dims: Option<String>,
    /// Comma-separated `type:name` extensions the producer ran with
    ext: Option<String>,
}

fn split_csv(value: &Option<String>) -> Vec<String> {
    value
        .iter()
        .flat_map(|v| v.split(','))
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub async fn get_unit_state(
    State(state): State<Arc<AppState>>,
    Path((org, name)): Path<(String, String)>,
    Query(params): Query<UnitStateQuery>,
) -> Result<Json<UnitStateRecord>, ApiError> {
    // `org`/`name` come straight from the URL path, `dims`/`ext` from the
    // query string - `UnitStateKey::try_new` is the v3 seam validation
    // (docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4) that rejects a
    // request like `GET /v1/../../etc/units/x/state` before it ever
    // reaches the filesystem adapter.
    let key = UnitStateKey::try_new(&org, &name, split_csv(&params.dims), split_csv(&params.ext))
        .map_err(AppError::from)?;
    let record = state
        .unit_state
        .get(&key)
        .await?
        .ok_or_else(|| AppError::not_found("unit state", key.canonical()))?;
    Ok(Json(record))
}
