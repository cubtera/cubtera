//! Run command

use clap::Args;
use cubtera_config::Config;
use cubtera_core::App;
use cubtera_persistence::Repositories;
use cubtera_runners::DefaultRunnerFactory;
use std::sync::Arc;

#[derive(Args)]
pub struct RunArgs {
    /// Unit name
    #[arg(short, long)]
    pub unit: String,

    /// Dimension (format: type:name), can be specified multiple times
    #[arg(short, long = "dim")]
    pub dimensions: Vec<String>,

    /// Auto-approve (skip confirmation)
    #[arg(long)]
    pub auto_approve: bool,

    /// Command to run (after --)
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(config: &Config, args: RunArgs) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config)?;
    let runner_factory = Arc::new(DefaultRunnerFactory::new());

    let app = App::new(repos.dimensions, repos.units, runner_factory, None);

    // Build unit
    let unit = app
        .units
        .build_unit(&config.org, &args.unit, &args.dimensions)
        .await?;

    println!("Running unit: {}", unit.name);
    println!("Dimensions: {:?}", unit.dimensions.iter().map(|d| d.key()).collect::<Vec<_>>());
    println!("Command: {:?}", args.command);
    println!();

    // Execute
    let result = app
        .runners
        .run(&unit, args.command, args.auto_approve)
        .await?;

    // Output results
    if !result.stdout.is_empty() {
        print!("{}", result.stdout);
    }
    if !result.stderr.is_empty() {
        eprint!("{}", result.stderr);
    }

    if result.is_success() {
        println!("\nCompleted successfully in {}ms", result.duration_ms);
        Ok(())
    } else {
        eprintln!("\nFailed with exit code {} in {}ms", result.exit_code, result.duration_ms);
        std::process::exit(result.exit_code);
    }
}

