//! Server setup

use crate::routes;
use axum::Router;
use cubtera_config::Config;
use cubtera_core::ports::{CopyConfig, DeploymentLogRepository};
use cubtera_core::App;
use cubtera_persistence::fs::FsWorkspace;
use cubtera_persistence::Repositories;
use cubtera_runners::{DefaultRunnerFactory, TokioProcessRunner};
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing::info;

/// Application state shared across handlers - routes go through [`App`]'s
/// services (never bare repositories), so API responses get the same
/// defaults gap-fill/parent-chain/access-policy behavior as the CLI. See
/// the migration plan, item 7.
pub struct AppState {
    pub app: App,
    /// Kept alongside `app` (rather than only inside `app.runners`, which
    /// only writes to it after a run) so `/v1/{org}/dlog` can read it
    /// directly.
    pub deployment_log: Arc<dyn DeploymentLogRepository>,
    /// `None` disables auth (local dev default) - see `crate::auth`.
    pub api_key: Option<String>,
}

pub async fn run(addr: &str, config: Config) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(&config).await?;
    let hierarchy = Repositories::hierarchy(&config);
    let deployment_log = repos.deployment_log.clone();

    let copy_config = CopyConfig {
        modules_path: config.modules_path.clone(),
        plugins_path: config.plugins_path.clone(),
        always_copy_files: config.always_copy_files,
        clean_cache: config.clean_cache,
    };

    let app = App::new(
        repos.inventory,
        hierarchy,
        repos.units,
        Arc::new(DefaultRunnerFactory::new()),
        Arc::new(FsWorkspace::new()),
        Arc::new(TokioProcessRunner::new()),
        copy_config,
        Some(repos.deployment_log),
    );

    if config.api_key.is_none() {
        tracing::warn!(
            "CUBTERA_API_KEY is not set - /v1 routes are unauthenticated. Set it in production."
        );
    }

    let state = Arc::new(AppState {
        app,
        deployment_log,
        api_key: config.api_key.clone(),
    });

    let app_router = Router::new()
        .merge(routes::router(state.clone()))
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("Starting Cubtera API server on {}", addr);

    axum::serve(listener, app_router).await?;

    Ok(())
}
