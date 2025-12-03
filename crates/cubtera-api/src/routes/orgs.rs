//! Organization endpoints

use crate::server::AppState;
use axum::{extract::State, http::StatusCode, Json};
use cubtera_core::services::DimensionService;
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn list_orgs(State(state): State<Arc<AppState>>) -> Result<Json<Value>, StatusCode> {
    let service = DimensionService::new(state.repos.dimensions.clone());

    match service.get_orgs().await {
        Ok(orgs) => Ok(Json(json!({
            "status": "ok",
            "data": orgs
        }))),
        Err(e) => {
            tracing::error!("Failed to list orgs: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

