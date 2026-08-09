//! Cubtera MCP server
//!
//! Exposes read-only inventory/unit/deployment-log queries as Model Context
//! Protocol tools over stdio, so MCP clients (IDEs, agents) can look up
//! Cubtera's dimensions and manifests directly. This is a proper MCP
//! server (via the `rmcp` SDK), not the REST-shaped prototype `test1` had -
//! see the migration plan's wave 2 note on `cubtera-mcp`.

mod error;
mod server;

use clap::Parser;
use cubtera_core::services::{DimensionService, UnitService};
use cubtera_persistence::Repositories;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use server::CubteraMcp;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "cubtera-mcp")]
#[command(author, version, about = "Cubtera MCP server")]
struct Cli {
    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // MCP over stdio uses stdout for protocol messages - logs must never go
    // there, or they'd corrupt the stream from the client's perspective.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive(tracing::Level::INFO.into()))
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let cli = Cli::parse();

    let config = match &cli.config {
        Some(path) => cubtera_config::Config::load_from_path(std::path::Path::new(path)),
        None => cubtera_config::Config::load(),
    }?;

    let repos = Repositories::from_config(&config).await?;
    let hierarchy = Repositories::hierarchy(&config);

    let dimensions = Arc::new(DimensionService::new(repos.inventory, hierarchy));
    let unit_state = repos.unit_state.clone();
    let units = Arc::new(
        UnitService::new(repos.units, dimensions.clone()).with_unit_state(repos.unit_state),
    );

    tracing::info!("Starting Cubtera MCP server");

    let service = CubteraMcp::new(dimensions, units, repos.deployment_log, unit_state)
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
