//! `cubtera run` (v3, P7-retire-run) - the "just run it" escape hatch:
//! resolve the unit and execute `command` directly through
//! `RunUseCase::apply_direct` - no `Plan` artifact, no pin-drift gate.
//! This is the v3-native replacement for v2's unconditional `run.rs`
//! (`cubtera_core::App`/`cubtera_persistence::fs::FsWorkspace`/
//! `cubtera_runners`) - no `cubtera-core`/`cubtera-domain`/
//! `cubtera-persistence`/`cubtera-runners` dependency left in this module.
//! Kept for backward-compatible CLI UX (`-u`/`-d`/`-e`/`--auto-approve`/
//! `--dry-run`/`-- <command>`) and because bash/helm runners have no plan
//! concept at all (`RunUseCase::plan` rejects them) - `cubtera plan` +
//! `cubtera apply --plan` remain the additional, opt-in reviewed-plan
//! workflow for tf/tofu units that want it.

use super::run_support::{build_input_requests, build_unit, config_digest, default_actor, prepare};
use super::Ctx;
use clap::Args;
use cubtera_app::ApplyRequest;
use cubtera_config::Config;
use cubtera_kernel::Ident;
use cubtera_model::RunStatus;
use std::time::Duration;

#[derive(Args)]
pub struct RunArgs {
    /// Unit name
    #[arg(short, long)]
    pub unit: String,

    /// Dimension (format: type:name), can be specified multiple times
    #[arg(short, long = "dim")]
    pub dimensions: Vec<String>,

    /// Extension (format: type:name), can be specified multiple times.
    /// Runs the same unit/dimensions with a different state (e.g. `-e index:0`).
    #[arg(short = 'e', long = "ext")]
    pub extensions: Vec<String>,

    /// Auto-approve (skip confirmation)
    #[arg(long)]
    pub auto_approve: bool,

    /// Print the materialization plan instead of executing it
    #[arg(long)]
    pub dry_run: bool,

    /// Who's running this. Defaults to `$USER`.
    #[arg(long)]
    pub actor: Option<String>,

    /// How long to hold the mutual-exclusion lease on this instance while
    /// running, in seconds.
    #[arg(long, default_value = "300")]
    pub lease_ttl_seconds: u64,

    /// Schema version to tag this run's published `[outputs]` with, if the
    /// manifest declares `[outputs] publish = true`.
    #[arg(long, default_value = "1.0.0")]
    pub outputs_schema_version: String,

    /// Command to run (after --)
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    args: RunArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    if args.dry_run {
        let unit = build_unit(config, &args.unit, &args.dimensions, &args.extensions).await?;
        let plan = unit.materialize(&config.modules_path, None)?;
        println!("{plan}");
        return Ok(());
    }

    if args.command.is_empty() {
        return Err(
            "cubtera run requires a command after `--` (e.g. `cubtera run -u X -d ... -- apply`)"
                .into(),
        );
    }

    let prepared = prepare(config, &args.unit, &args.dimensions, &args.extensions).await?;
    let runner_type = prepared.unit.manifest.runner_type().as_str().to_string();
    let publish_outputs = prepared
        .unit
        .manifest
        .outputs
        .as_ref()
        .map(|o| o.publish)
        .unwrap_or(false);
    let actor = args.actor.clone().unwrap_or_else(default_actor);
    let outputs_schema_version =
        semver::Version::parse(&args.outputs_schema_version).map_err(|e| {
            format!(
                "invalid --outputs-schema-version {:?}: {e}",
                args.outputs_schema_version
            )
        })?;
    let inputs = build_input_requests(config, &prepared.unit).await?;

    println!("Running unit: {}", prepared.unit.name);
    println!(
        "Dimensions: {:?}",
        prepared
            .unit
            .dimensions
            .iter()
            .map(|d| d.key())
            .collect::<Vec<_>>()
    );
    println!("Command: {:?}", args.command);
    println!("Temp folder: {}", prepared.unit.temp_folder.display());
    println!();

    let run_record = prepared
        .use_case
        .apply_direct(
            prepared.instance.clone(),
            ApplyRequest {
                runner_type,
                command: args.command.clone(),
                auto_approve: args.auto_approve,
                actor: Ident::parse(&actor)?,
                config_digest: config_digest(config)?,
                publish_outputs,
                outputs_schema_version,
                lease_ttl: Duration::from_secs(args.lease_ttl_seconds),
                inputs,
            },
        )
        .await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&run_record)?);
    } else {
        println!("Run:          {}", run_record.id);
        println!("Status:       {:?}", run_record.status);
        println!("Exit code:    {}", run_record.exit_code.unwrap_or(-1));
        if let Some(revision) = run_record.produced_outputs_revision {
            println!("Outputs rev:  {revision}");
        }
        match run_record.status {
            RunStatus::Succeeded => println!("\nCompleted successfully"),
            _ => eprintln!(
                "\nFailed with exit code {}",
                run_record.exit_code.unwrap_or(1)
            ),
        }
    }

    if run_record.status != RunStatus::Succeeded {
        std::process::exit(run_record.exit_code.unwrap_or(1));
    }

    Ok(())
}
