//! Unit manifest endpoints - `cubtera-api`'s `units.rs`, same seam
//! rationale as `inventory.rs`.

use crate::error::ApiError;
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::Json;
use cubtera_core::services::{DimensionService, UnitService};
use cubtera_persistence::Repositories;
use serde_json::{json, Value};
use std::sync::Arc;

async fn unit_service(state: &AppState) -> Result<UnitService, ApiError> {
    let repos = Repositories::from_config(&state.config)
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?;
    let hierarchy = Repositories::hierarchy(&state.config);
    let dimensions = Arc::new(DimensionService::new(repos.inventory, hierarchy));
    Ok(UnitService::new(repos.units, dimensions))
}

pub async fn list_units(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let units = unit_service(&state).await?;
    let names = units.list_units(&org).await?;
    Ok(Json(json!(names)))
}

pub async fn get_unit(
    State(state): State<Arc<AppState>>,
    Path((org, name)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let units = unit_service(&state).await?;
    let manifest = units.get_manifest(&org, &name).await?;
    Ok(Json(json!(manifest)))
}
