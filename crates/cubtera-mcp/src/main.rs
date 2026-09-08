//! Cubtera MCP server
//!
//! Exposes read-only inventory/unit/deployment-log queries as Model Context
//! Protocol tools over stdio, so MCP clients (IDEs, agents) can look up
//! Cubtera's dimensions and manifests directly. This is a proper MCP
//! server (via the `rmcp` SDK), not the REST-shaped prototype `test1` had.
//!
//! Since P7, this is a pure HTTP client of `cubtera-server` (see
//! `client.rs`) - it has no `cubtera-core`/`cubtera-persistence` dependency
//! and never touches the filesystem or a store directly. Point it at a
//! running `cubtera-server` with `--server-url`/`CUBTERA_SERVER_URL`.

mod client;
mod error;
mod server;

use clap::Parser;
use client::CubteraApiClient;
use rmcp::transport::stdio;
use rmcp::ServiceExt;
use server::CubteraMcp;
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "cubtera-mcp")]
#[command(author, version, about = "Cubtera MCP server")]
struct Cli {
    /// Base URL of a running cubtera-server (env: CUBTERA_SERVER_URL)
    #[arg(
        long,
        env = "CUBTERA_SERVER_URL",
        default_value = "http://127.0.0.1:8081"
    )]
    server_url: String,

    /// API key to send as `x-api-key` (env: CUBTERA_API_KEY)
    #[arg(long, env = "CUBTERA_API_KEY")]
    api_key: Option<String>,
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

    tracing::info!(
        "Starting Cubtera MCP server (proxying cubtera-server at {})",
        cli.server_url
    );

    let client = CubteraApiClient::new(cli.server_url, cli.api_key);
    let service = CubteraMcp::new(client)
        .serve(stdio())
        .await
        .inspect_err(|e| {
            tracing::error!("serving error: {:?}", e);
        })?;

    service.waiting().await?;
    Ok(())
}
