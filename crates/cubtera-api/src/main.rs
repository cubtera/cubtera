//! Cubtera REST API Server

mod auth;
mod error;
mod routes;
mod server;

use cubtera_config::Config;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Setup logging
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = Config::load()?;

    // Start server
    let addr = std::env::var("CUBTERA_API_ADDR").unwrap_or_else(|_| "0.0.0.0:8080".to_string());
    server::run(&addr, config).await
}
