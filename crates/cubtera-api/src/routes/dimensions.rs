//! Dimension endpoints

use crate::server::AppState;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    Json,
};
use cubtera_core::services::DimensionService;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn list_dim_types(
    State(state): State<Arc<AppState>>,
    Path(org): Path<String>,
) -> Result<Json<Value>, StatusCode> {
    let service = DimensionService::new(state.repos.dimensions.clone());

    match service.get_types(&org).await {
        Ok(types) => Ok(Json(json!({
            "status": "ok",
            "org": org,
            "data": types
        }))),
        Err(e) => {
            tracing::error!("Failed to list dim types: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn list_dimensions(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let service = DimensionService::new(state.repos.dimensions.clone());

    match service.get_all_names(&org, &dim_type).await {
        Ok(names) => Ok(Json(json!({
            "status": "ok",
            "org": org,
            "type": dim_type,
            "data": names
        }))),
        Err(e) => {
            tracing::error!("Failed to list dimensions: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn get_dimension(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type, name)): Path<(String, String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let service = DimensionService::new(state.repos.dimensions.clone());

    match service.get_by_name(&org, &dim_type, &name).await {
        Ok(dim) => Ok(Json(json!({
            "status": "ok",
            "org": org,
            "type": dim_type,
            "name": name,
            "parent": dim.parent_ref,
            "data": {}  // TODO: Convert domain Value to JSON
        }))),
        Err(e) => {
            tracing::error!("Failed to get dimension: {}", e);
            Err(StatusCode::NOT_FOUND)
        }
    }
}

pub async fn get_defaults(
    State(state): State<Arc<AppState>>,
    Path((org, dim_type)): Path<(String, String)>,
) -> Result<Json<Value>, StatusCode> {
    let service = DimensionService::new(state.repos.dimensions.clone());

    match service.get_defaults(&org, &dim_type).await {
        Ok(Some(dim)) => Ok(Json(json!({
            "status": "ok",
            "org": org,
            "type": dim_type,
            "data": {
                "name": dim.name,
                "parent": dim.parent_ref
            }
        }))),
        Ok(None) => Ok(Json(json!({
            "status": "ok",
            "org": org,
            "type": dim_type,
            "data": null
        }))),
        Err(e) => {
            tracing::error!("Failed to get defaults: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

