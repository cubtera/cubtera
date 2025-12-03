//! Cubtera CLI
//!
//! Multi-dimensional Infrastructure Manager

mod commands;

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
async fn main() -> Result<(), Box<dyn std::error::Error>> {
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
    tracing::subscriber::set_global_default(subscriber)?;

    // Load config
    let config = if let Some(path) = cli.config {
        cubtera_config::Config::load_from_path(std::path::Path::new(&path))?
    } else {
        cubtera_config::Config::load()?
    };

    // Execute command
    match cli.command {
        Commands::Config => commands::config::run(&config),
        Commands::Im(cmd) => commands::im::run(&config, cmd).await,
        Commands::Run(args) => commands::run::run(&config, args).await,
        Commands::Log(cmd) => commands::log::run(&config, cmd).await,
    }
}

