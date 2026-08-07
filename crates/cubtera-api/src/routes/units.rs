//! Unit endpoints

use crate::error::ApiError;
use crate::server::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn list_units(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let units = state.app.units.list_units(&org).await?;
    Ok(Json(json!(units)))
}

pub async fn get_unit(
    State(state): State<Arc<AppState>>,
    Path((org, name)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let manifest = state.app.units.get_manifest(&org, &name).await?;
    Ok(Json(json!(manifest)))
}
