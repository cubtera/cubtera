//! `GET /v1/{org}/state?unit=..&dims=..&ext=..`,
//! `GET /v1/{org}/state/stale` - v3 state-mesh reads (P6), straight out of
//! `Store::get_output_set`/`Store::list_stale_consumers`. Distinct from
//! `cubtera-api`'s `GET /v1/{org}/units/{name}/state`, which reads v2's
//! legacy `UnitStateRepository` - this reads the real `OutputSet` (schema
//! version, revision, per-value `OutputValue::Plain`/`::Secret`) `plan`/
//! `apply`'s `[inputs]` resolution actually consumes.

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use cubtera_kernel::{DimRef, Ident, InstanceId};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct StateQuery {
    unit: String,
    #[serde(default)]
    dims: Option<String>,
    #[serde(default)]
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

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
    Query(params): Query<StateQuery>,
) -> Result<Json<Value>, ApiError> {
    let dims: Vec<DimRef> = split_csv(&params.dims)
        .iter()
        .map(|s| DimRef::parse(s))
        .collect::<Result<_, _>>()
        .map_err(cubtera_app::AppError::from)?;
    let ext: Vec<DimRef> = split_csv(&params.ext)
        .iter()
        .map(|s| DimRef::parse(s))
        .collect::<Result<_, _>>()
        .map_err(cubtera_app::AppError::from)?;
    let instance = InstanceId::try_new(
        Ident::parse(&org).map_err(cubtera_app::AppError::from)?,
        Ident::parse(&params.unit).map_err(cubtera_app::AppError::from)?,
        dims,
        ext,
    )
    .map_err(cubtera_app::AppError::from)?;

    let store = open_store(&state)?;
    let record = store
        .get_output_set(&instance)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
        .ok_or_else(|| {
            ApiError::not_found(format!("no published state for {}", instance.canonical()))
        })?;

    Ok(Json(json!(record)))
}

pub async fn stale(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let org = Ident::parse(&org).map_err(cubtera_app::AppError::from)?;
    let store = open_store(&state)?;
    let consumers = store
        .list_stale_consumers(&org)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    Ok(Json(json!(consumers)))
}

fn open_store(state: &AppState) -> Result<Arc<dyn cubtera_store::Store>, ApiError> {
    Ok(Arc::new(
        cubtera_store::SqliteStore::open(&state.config.store_path)
            .map_err(|e| ApiError::bad_request(format!("failed to open store: {e}")))?,
    ))
}
