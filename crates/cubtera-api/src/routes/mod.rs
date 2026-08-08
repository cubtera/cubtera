//! API routes

mod dimensions;
mod dlog;
mod health;
mod orgs;
mod units;

use crate::auth::require_api_key;
use crate::server::AppState;
use axum::{middleware, routing::get, Router};
use std::sync::Arc;

pub fn router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/v1", v1_router(state))
}

fn v1_router(state: Arc<AppState>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/orgs", get(orgs::list_orgs))
        .route("/{org}/dim-types", get(dimensions::list_dim_types))
        .route("/{org}/dims/{dim_type}", get(dimensions::list_dimensions))
        // NOTE: static suffixes (/defaults, /schema) must be routed before
        // the dynamic /{name} segment so they aren't captured as a name.
        .route(
            "/{org}/dims/{dim_type}/defaults",
            get(dimensions::get_defaults),
        )
        .route("/{org}/dims/{dim_type}/schema", get(dimensions::get_schema))
        .route(
            "/{org}/dims/{dim_type}/{name}",
            get(dimensions::get_dimension),
        )
        .route(
            "/{org}/dims/{dim_type}/{name}/parent",
            get(dimensions::get_parent),
        )
        .route(
            "/{org}/dims/{dim_type}/{name}/children",
            get(dimensions::get_children),
        )
        .route("/{org}/units", get(units::list_units))
        .route("/{org}/units/{name}", get(units::get_unit))
        .route("/{org}/dlog", get(dlog::get_logs))
        // Auth applies to every /v1 route but not /health (used for
        // liveness probes, which shouldn't need a key) - see `crate::auth`.
        .route_layer(middleware::from_fn_with_state(state, require_api_key))
}
