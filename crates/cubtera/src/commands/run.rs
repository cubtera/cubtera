//! Run command

use clap::Args;
use cubtera_config::Config;
use cubtera_core::ports::CopyConfig;
use cubtera_core::App;
use cubtera_domain::RunParams;
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

    // Build CopyConfig from global config
    let copy_config = CopyConfig {
        modules_path: config.modules_path.clone(),
        plugins_path: config.plugins_path.clone(),
        always_copy_files: config.always_copy_files,
        clean_cache: config.clean_cache,
    };

    let app = App::new(
        repos.dimensions,
        repos.units,
        runner_factory,
        copy_config,
        None,
    );

    // Build unit
    let mut unit = app
        .units
        .build_unit(&config.org, &args.unit, &args.dimensions)
        .await?;

    // Calculate and set temp folder
    let temp_folder = unit.calculate_temp_folder(&config.temp_folder_path);
    unit = unit.with_temp_folder(temp_folder);

    println!("Running unit: {}", unit.name);
    println!(
        "Dimensions: {:?}",
        unit.dimensions.iter().map(|d| d.key()).collect::<Vec<_>>()
    );
    println!("Command: {:?}", args.command);
    println!("Temp folder: {}", unit.temp_folder.display());
    println!();

    // Build params
    let params = if args.auto_approve {
        Some(
            RunParams::new(&unit.temp_folder)
                .with_commands(args.command.clone())
                .with_auto_approve(true),
        )
    } else {
        None
    };

    // Execute
    let result = app
        .runners
        .run(&unit, args.command, params)
        .await?;

    // Output results
    if let Some(output) = &result.output {
        print!("{}", output);
    }

    let exit_code = result.exit_code.unwrap_or(0);

    if result.is_success() {
        println!("\nCompleted successfully");
        Ok(())
    } else {
        eprintln!("\nFailed with exit code {}", exit_code);
        std::process::exit(exit_code);
    }
}
