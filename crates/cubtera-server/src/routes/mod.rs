//! Server routes.

mod fleet;
mod health;
mod run;
mod state;
mod validate;

use crate::auth::require_api_key;
use crate::server::AppState;
use axum::{middleware, routing::get, routing::post, Router};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/v1", v1_router(state))
}

fn v1_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/{org}/validate", get(validate::validate))
        .route("/{org}/fleet/status", get(fleet::status))
        .route("/{org}/units/{unit}/plan", post(run::plan))
        .route("/{org}/units/{unit}/apply", post(run::apply))
        .route("/{org}/runs/{run_id}", get(run::explain))
        .route("/{org}/runs/{run_id}/log", get(run::log))
        .route("/{org}/state", get(state::get))
        .route("/{org}/state/stale", get(state::stale))
        // Auth applies to every /v1 route but not /health, same as
        // `cubtera-api` - see `crate::auth`.
        .route_layer(middleware::from_fn_with_state(state, require_api_key))
}
