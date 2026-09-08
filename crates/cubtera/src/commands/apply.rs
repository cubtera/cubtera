//! `cubtera apply --plan <plan_id>` (v3, P4-run) - the approval gate: load
//! a previously reviewed `Plan`, refuse if it expired or its pins no
//! longer match a fresh resolution, take a lease on the instance, run, and
//! (if the manifest asked for it and the runner can) publish an
//! `OutputSet`. See `cubtera_app::run::RunUseCase::apply`.
//!
//! `cubtera apply -s "env:prod" --waves` (selector-driven, multi-instance
//! apply) is `Binding`/P5 scope - this command is the single-instance
//! primitive P5's batches will eventually call.

use super::run_support::{build_input_requests, config_digest, default_actor, prepare};
use super::Ctx;
use clap::Args;
use cubtera_app::ApplyRequest;
use cubtera_config::Config;
use cubtera_kernel::Ident;
use cubtera_model::{PlanId, RunStatus};
use std::time::Duration;

#[derive(Args)]
pub struct ApplyArgs {
    /// Unit name - must match the unit `--plan` was created for
    #[arg(short, long)]
    pub unit: String,

    /// Dimension (format: type:name) - must match the dimensions `--plan`
    /// was created for, so the recomputed `InstanceId` agrees with the
    /// plan's
    #[arg(short, long = "dim")]
    pub dimensions: Vec<String>,

    /// Extension (format: type:name), can be specified multiple times
    #[arg(short = 'e', long = "ext")]
    pub extensions: Vec<String>,

    /// The `Plan` id to apply - printed by `cubtera plan`
    #[arg(long = "plan")]
    pub plan_id: String,

    /// Skip interactive confirmation prompts in the underlying runner
    #[arg(long)]
    pub auto_approve: bool,

    /// Who's running this apply. Defaults to `$USER`.
    #[arg(long)]
    pub actor: Option<String>,

    /// How long to hold the mutual-exclusion lease on this instance while
    /// applying, in seconds.
    #[arg(long, default_value = "300")]
    pub lease_ttl_seconds: u64,

    /// Schema version to tag this run's published `[outputs]` with, if the
    /// manifest declares `[outputs] publish = true`.
    #[arg(long, default_value = "1.0.0")]
    pub outputs_schema_version: String,

    /// Command to run (after `--`); defaults to `apply`.
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    args: ApplyArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let prepared = prepare(config, &args.unit, &args.dimensions, &args.extensions).await?;
    let runner_type = prepared.unit.manifest.runner_type().as_str().to_string();
    let command = if args.command.is_empty() {
        vec!["apply".to_string()]
    } else {
        args.command.clone()
    };
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

    let run = prepared
        .use_case
        .apply(
            &PlanId::new(args.plan_id.clone()),
            ApplyRequest {
                runner_type,
                command,
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
        println!("{}", serde_json::to_string_pretty(&run)?);
    } else {
        println!("Run:          {}", run.id);
        println!("Status:       {:?}", run.status);
        println!("Exit code:    {}", run.exit_code.unwrap_or(-1));
        if let Some(revision) = run.produced_outputs_revision {
            println!("Outputs rev:  {revision}");
        }
    }

    if run.status != RunStatus::Succeeded {
        std::process::exit(run.exit_code.unwrap_or(1));
    }

    Ok(())
}
