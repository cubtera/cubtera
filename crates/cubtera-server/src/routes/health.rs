//! Liveness probe - unauthenticated, same as `cubtera-api`'s.

use axum::Json;
use serde_json::{json, Value};

pub async fn health_check() -> Json<Value> {
    Json(json!({"status": "ok"}))
}
