//! Server setup

use crate::routes;
use axum::Router;
use cubtera_config::Config;
use cubtera_persistence::Repositories;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Application state shared across handlers
pub struct AppState {
    pub config: Config,
    pub repos: Repositories,
}

pub async fn run(addr: &str, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(&config)?;

    let state = Arc::new(AppState { config, repos });

    let app = Router::new()
        .nest("/", routes::router())
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Starting Cubtera API server on {}", addr);

    axum::serve(listener, app).await?;

    Ok(())
}

