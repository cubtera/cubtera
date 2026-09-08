//! Unit manifest endpoints - v3-native, `cubtera_app::AssembleUseCase`
//! (`FsUnitPort`) instead of v2's `UnitService`/`Repositories`.

use crate::error::ApiError;
use crate::run_support::{inventory_port, unit_port};
use crate::server::AppState;
use axum::extract::{Path, State};
use axum::Json;
use cubtera_app::AssembleUseCase;
use serde_json::{json, Value};
use std::sync::Arc;

fn assemble(state: &AppState) -> AssembleUseCase {
    AssembleUseCase::new(inventory_port(&state.config), unit_port(&state.config))
}

pub async fn list_units(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let names = assemble(&state).list_units(&org).await?;
    Ok(Json(json!(names)))
}

pub async fn get_unit(
    State(state): State<Arc<AppState>>,
    Path((org, name)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let manifest = assemble(&state).get_manifest(&org, &name).await?;
    Ok(Json(json!(manifest)))
}
