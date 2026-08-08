//! Cubtera CLI
//!
//! Multi-dimensional Infrastructure Manager

mod commands;
mod error;

use clap::{Parser, Subcommand};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "cubtera")]
#[command(author, version, about = "Multi-dimensional Infrastructure Manager")]
struct Cli {
    /// Log level (error, warn, info, debug, trace)
    #[arg(long, global = true, default_value = "info")]
    log_level: String,

    /// Configuration file path
    #[arg(short, long, global = true)]
    config: Option<String>,

    /// Emit machine-readable JSON instead of human-readable text
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Show current configuration
    Config,

    /// Inventory management commands
    #[command(subcommand)]
    Im(commands::im::ImCommands),

    /// Run a unit
    Run(commands::run::RunArgs),

    /// Deployment log commands
    #[command(subcommand)]
    Log(commands::log::LogCommands),
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    // Setup logging
    let level = match cli.log_level.to_lowercase().as_str() {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => Level::INFO,
    };

    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .finish();
    if tracing::subscriber::set_global_default(subscriber).is_err() {
        eprintln!("Warning: failed to install tracing subscriber");
    }

    // Load config
    let config = match &cli.config {
        Some(path) => cubtera_config::Config::load_from_path(std::path::Path::new(path)),
        None => cubtera_config::Config::load(),
    };
    let config = match config {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Configuration error: {e}");
            std::process::exit(error::EXIT_CONFIG);
        }
    };

    let ctx = commands::Ctx { json: cli.json };

    // Execute command
    let result = match cli.command {
        Commands::Config => commands::config::run(&config, &ctx),
        Commands::Im(cmd) => commands::im::run(&config, &ctx, cmd).await,
        Commands::Run(args) => commands::run::run(&config, args).await,
        Commands::Log(cmd) => commands::log::run(&config, &ctx, cmd).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(error::exit_code_for(e.as_ref()));
    }
}
