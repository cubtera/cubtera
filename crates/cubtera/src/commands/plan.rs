//! `cubtera plan -u <unit> -d <dims...>` (v3, P4-run) - resolve everything,
//! run the plan step through a capability-checked runner, and persist the
//! result as a reviewable `Plan` artifact `apply --plan <id>` can later
//! replay a pin check against. See `cubtera_app::run::RunUseCase::plan`.

use super::run_support::{build_input_requests, config_digest, default_actor, prepare};
use super::Ctx;
use clap::Args;
use cubtera_app::PlanRequest;
use cubtera_config::Config;
use cubtera_kernel::Ident;

#[derive(Args)]
pub struct PlanArgs {
    /// Unit name
    #[arg(short, long)]
    pub unit: String,

    /// Dimension (format: type:name), can be specified multiple times
    #[arg(short, long = "dim")]
    pub dimensions: Vec<String>,

    /// Extension (format: type:name), can be specified multiple times
    #[arg(short = 'e', long = "ext")]
    pub extensions: Vec<String>,

    /// Who's requesting this plan (`Run`/`Plan` provenance). Defaults to
    /// `$USER`.
    #[arg(long)]
    pub actor: Option<String>,

    /// How long the resulting plan stays valid for `apply --plan` before
    /// it must be recomputed, in seconds.
    #[arg(long, default_value = "3600")]
    pub ttl_seconds: i64,

    /// Command to run for the plan step (after `--`); defaults to `plan`.
    #[arg(last = true)]
    pub command: Vec<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    args: PlanArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let prepared = prepare(config, &args.unit, &args.dimensions, &args.extensions).await?;
    let runner_type = prepared.unit.manifest.runner_type().as_str().to_string();
    let command = if args.command.is_empty() {
        vec!["plan".to_string()]
    } else {
        args.command.clone()
    };
    let actor = args.actor.clone().unwrap_or_else(default_actor);
    let inputs = build_input_requests(config, &prepared.unit).await?;

    let plan = prepared
        .use_case
        .plan(PlanRequest {
            instance: prepared.instance.clone(),
            runner_type,
            command,
            actor: Ident::parse(&actor)?,
            config_digest: config_digest(config)?,
            ttl_seconds: args.ttl_seconds,
            inputs,
        })
        .await?;

    if ctx.json {
        println!("{}", serde_json::to_string_pretty(&plan)?);
    } else {
        println!("Plan:          {}", plan.id);
        println!("Instance:      {}", plan.instance.canonical());
        println!("Diff:          {}", plan.diff_summary);
        println!("Runner version: {}", plan.resolution.runner_version);
        println!(
            "Package digest: {}",
            plan.resolution.package_digest.to_hex()
        );
        println!("Expires at:    {} (unix ms)", plan.expires_at);
        println!();
        println!(
            "apply with: cubtera apply -u {} --plan {}",
            args.unit, plan.id
        );
    }

    Ok(())
}
