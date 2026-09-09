//! Server routes.

mod dlog;
mod fleet;
mod health;
mod inventory;
mod run;
mod state;
mod units;
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
        .route("/orgs", get(inventory::list_orgs))
        .route("/{org}/dim-types", get(inventory::list_dim_types))
        .route(
            "/{org}/dims/{dim_type}",
            get(inventory::list_dimension_names),
        )
        .route(
            "/{org}/dims/{dim_type}/defaults",
            get(inventory::get_defaults),
        )
        .route("/{org}/dims/{dim_type}/schema", get(inventory::get_schema))
        .route(
            "/{org}/dims/{dim_type}/{name}",
            get(inventory::get_dimension),
        )
        .route(
            "/{org}/dims/{dim_type}/{name}/parent",
            get(inventory::get_parent),
        )
        .route(
            "/{org}/dims/{dim_type}/{name}/children",
            get(inventory::get_children),
        )
        .route(
            "/{org}/dims/{dim_type}/{name}/validate",
            get(inventory::validate_dimension),
        )
        .route("/{org}/units", get(units::list_units))
        .route("/{org}/units/{name}", get(units::get_unit))
        .route("/{org}/dlog", get(dlog::get_deployment_log))
        .route("/{org}/validate", get(validate::validate))
        .route("/{org}/fleet/status", get(fleet::status))
        .route("/{org}/units/{unit}/plan", post(run::plan))
        .route("/{org}/units/{unit}/apply", post(run::apply))
        .route("/{org}/runs/{run_id}", get(run::explain))
        .route("/{org}/runs/{run_id}/log", get(run::log))
        .route("/{org}/runs/{run_id}/log/stream", get(run::log_stream))
        .route("/{org}/state", get(state::get))
        .route("/{org}/state/stale", get(state::stale))
        // Auth applies to every /v1 route but not /health, same as
        // `cubtera-api` - see `crate::auth`.
        .route_layer(middleware::from_fn_with_state(state, require_api_key))
}
