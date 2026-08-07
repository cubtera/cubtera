//! Dimension endpoints
//!
//! Plain resource responses (no `{status, id, data}` envelope - see the
//! migration plan's "что осознанно не переносим из v1").

use crate::error::ApiError;
use crate::server::AppState;
use axum::{
    extract::{Path, State},
    Json,
};
use cubtera_domain::Dimension;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn list_dim_types(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let types = state.app.dimensions.get_types(&org).await?;
    Ok(Json(json!(types)))
}

pub async fn list_dimensions(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let names = state.app.dimensions.get_all_names(&org, &dim_type).await?;
    Ok(Json(json!(names)))
}

pub async fn get_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dim = state
        .app
        .dimensions
        .get_by_name(&org, &dim_type, &name)
        .await?;
    Ok(Json(dim.to_response_json()))
}

pub async fn get_defaults(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let dim = state.app.dimensions.get_defaults(&org, &dim_type).await?;
    Ok(Json(
        dim.as_ref()
            .map(Dimension::to_response_json)
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_schema(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, ApiError> {
    let schema = state.app.dimensions.get_schema(&org, &dim_type).await?;
    Ok(Json(schema.unwrap_or(Value::Null)))
}

pub async fn get_parent(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let parent = state
        .app
        .dimensions
        .get_parent(&org, &dim_type, &name)
        .await?;
    Ok(Json(
        parent
            .as_ref()
            .map(Dimension::to_response_json)
            .unwrap_or(Value::Null),
    ))
}

pub async fn get_children(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, ApiError> {
    let children = state
        .app
        .dimensions
        .get_children(&org, &dim_type, &name)
        .await?;
    Ok(Json(json!(children
        .iter()
        .map(Dimension::to_response_json)
        .collect::<Vec<_>>())))
}
