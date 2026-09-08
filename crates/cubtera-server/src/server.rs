//! Server setup.

use crate::routes;
use axum::Router;
use cubtera_config::Config;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

pub struct AppState {
    pub config: Config,
    /// `None` disables auth (local dev default) - see `crate::auth`.
    pub api_key: Option<String>,
}

pub async fn run(addr: &str, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    if config.api_key.is_none() {
        tracing::warn!(
            "CUBTERA_API_KEY is not set - /v1 routes are unauthenticated. Set it in production."
        );
    }

    let state = std::sync::Arc::new(AppState {
        api_key: config.api_key.clone(),
        config,
    });

    let app_router = Router::new()
        .merge(routes::router(state.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!(
        "Starting Cubtera server (v3, cubtera-app-backed) on {}",
        addr
    );

    axum::serve(listener, app_router).await?;

    Ok(())
}
