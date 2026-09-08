//! `cubtera explain run <run_id>` (v3, P4-run) - look up one `Run` record
//! by id. Deliberately the smallest possible slice of "fleet visibility":
//! no instance/unit filter is needed since `RunId` is already globally
//! unique - `cubtera log get`/`cubtera state ls` remain the v2 tools for
//! browsing by unit/dims until P5-P6 fold them into this surface.

use super::Ctx;
use clap::{Args, Subcommand};
use cubtera_config::Config;
use cubtera_model::RunId;
use cubtera_persistence::Repositories;

#[derive(Subcommand)]
pub enum ExplainCommands {
    /// Explain one run by id
    Run(ExplainRunArgs),
}

#[derive(Args)]
pub struct ExplainRunArgs {
    /// The `Run` id to look up - printed by `cubtera apply`
    pub run_id: String,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: ExplainCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    match cmd {
        ExplainCommands::Run(args) => {
            let repos = Repositories::from_config(config).await?;
            // `explain` never executes anything, so the executor bridge's
            // workspace root is never touched - the configured temp folder
            // is a harmless placeholder.
            let use_case = super::run_support::build_use_case(
                config,
                &repos,
                config.temp_folder_path.clone(),
            )?;

            let found = use_case.explain(&RunId::new(args.run_id.clone())).await?;

            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&found)?);
            } else {
                println!("Run:          {}", found.id);
                println!("Instance:     {}", found.instance.canonical());
                println!("Op:           {:?}", found.op);
                println!("Status:       {:?}", found.status);
                println!("Actor:        {}", found.actor);
                println!("Started at:   {} (unix ms)", found.started_at);
                if let Some(finished) = found.finished_at {
                    println!("Finished at:  {finished} (unix ms)");
                }
                println!("Exit code:    {:?}", found.exit_code);
                if let Some(plan_ref) = &found.plan_ref {
                    println!("Plan:         {plan_ref}");
                }
                if let Some(revision) = found.produced_outputs_revision {
                    println!("Outputs rev:  {revision}");
                }
            }
        }
    }

    Ok(())
}
