//! Cubtera CLI
//!
//! Multi-dimensional Infrastructure Manager

mod commands;
mod error;
mod exec_bridge;

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

    /// Unit state (cross-unit outputs) commands
    #[command(subcommand)]
    State(commands::state::StateCommands),

    /// Static fleet-wide validation: schemas + dim-graph edges (v3, P3)
    Validate(commands::validate::ValidateArgs),

    /// Fleet visibility commands (v3, P3) - `Binding`/selectors land in P5
    #[command(subcommand)]
    Fleet(commands::fleet::FleetCommands),

    /// Resolve and run the plan step through a capability-checked runner,
    /// persisting the result as a reviewable `Plan` artifact (v3, P4)
    Plan(commands::plan::PlanArgs),

    /// Apply a previously created `Plan` (v3, P4) - the approval gate
    Apply(commands::apply::ApplyArgs),

    /// Explain a past `Run`/`Plan` (v3, P4)
    #[command(subcommand)]
    Explain(commands::explain::ExplainCommands),

    /// Diff a `Binding` (unit + selector) against `Store`, reporting only
    /// drift - package changes or orphaned instances (v3, P5)
    Drift(commands::drift::DriftArgs),

    /// Migrate a v1.x/v2 install onto v3: clean up dead `config.toml` keys
    /// and import historical fs-jsonl/fs-json dlog/unit-state data into the
    /// SQLite store (v3, P7)
    Migrate(commands::migrate::MigrateArgs),
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

    // Load config. `config_path` is the resolved file path (whether or not
    // it actually exists yet) - `cubtera migrate` needs it to read/rewrite
    // the raw TOML directly, mirroring `cubtera_config::Config::load`'s own
    // `$CUBTERA_CONFIG`-or-`~/.cubtera/config.toml` resolution so the two
    // never disagree on which file is "the" config.
    let config_path = cli
        .config
        .clone()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var("CUBTERA_CONFIG")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .or_else(|_| std::env::var("USERPROFILE"))
                        .map(|h| {
                            std::path::PathBuf::from(h)
                                .join(".cubtera")
                                .join("config.toml")
                        })
                        .unwrap_or_else(|_| std::path::PathBuf::from("config.toml"))
                })
        });
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
        Commands::Run(args) => commands::run::run(&config, &ctx, args).await,
        Commands::Log(cmd) => commands::log::run(&config, &ctx, cmd).await,
        Commands::State(cmd) => commands::state::run(&config, &ctx, cmd).await,
        Commands::Validate(args) => commands::validate::run(&config, &ctx, args).await,
        Commands::Fleet(cmd) => commands::fleet::run(&config, &ctx, cmd).await,
        Commands::Plan(args) => commands::plan::run(&config, &ctx, args).await,
        Commands::Apply(args) => commands::apply::run(&config, &ctx, args).await,
        Commands::Explain(cmd) => commands::explain::run(&config, &ctx, cmd).await,
        Commands::Drift(args) => commands::drift::run(&config, &ctx, args).await,
        Commands::Migrate(args) => commands::migrate::run(&config, &ctx, &config_path, args).await,
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(error::exit_code_for(e.as_ref()));
    }
}
