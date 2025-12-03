//! API routes

mod dimensions;
mod health;
mod orgs;

use crate::server::AppState;
use axum::{routing::get, Router};
use std::sync::Arc;

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/health", get(health::health_check))
        .nest("/v1", v1_router())
}

fn v1_router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/orgs", get(orgs::list_orgs))
        .route("/{org}/dim-types", get(dimensions::list_dim_types))
        .route("/{org}/dims/{dim_type}", get(dimensions::list_dimensions))
        .route(
            "/{org}/dims/{dim_type}/{name}",
            get(dimensions::get_dimension),
        )
        .route(
            "/{org}/dims/{dim_type}/defaults",
            get(dimensions::get_defaults),
        )
}

