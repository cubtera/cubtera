//! Organization endpoints

use crate::error::ApiError;
use crate::server::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn list_orgs(State(state): State<Arc<AppState>>) -> Result<Json<Value>, ApiError> {
    let orgs = state.app.dimensions.get_orgs().await?;
    Ok(Json(json!(orgs)))
}
