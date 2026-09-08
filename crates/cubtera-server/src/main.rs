//! Cubtera v3 HTTP API server - unlike `cubtera-api` (v2, read-only), this
//! can drive `plan`/`apply`/`explain` through `cubtera-app`. See this
//! crate's `Cargo.toml` description and `docs/specs/2026-09-03-cubtera-v3-architecture.md`
//! section 8 (P7).

mod app_bridge;
mod auth;
mod error;
mod exec_bridge;
mod policy;
mod routes;
mod run_support;
mod server;

use cubtera_config::Config;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let config = Config::load()?;

    let addr = std::env::var("CUBTERA_SERVER_ADDR").unwrap_or_else(|_| "0.0.0.0:8081".to_string());
    server::run(&addr, config).await
}
