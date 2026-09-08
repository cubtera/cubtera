//! `cubtera validate` - static check of the fleet: every dimension of
//! every configured type against its JSON schema (mandatory in v3, section 5.1)
//! and its dim-graph edge (v2 never checked either at more than one
//! dimension at a time). First real command built on `cubtera-app` (P3);
//! see docs/specs/2026-09-03-cubtera-v3-architecture.md section 3/section 10.

use super::Ctx;
use crate::app_bridge::InventoryPortBridge;
use clap::Args;
use cubtera_app::{load_dim_graph, ResolveUseCase, ValidateUseCase};
use cubtera_config::Config;
use cubtera_kernel::Ident;
use cubtera_model::ModelError;
use cubtera_persistence::Repositories;
use serde_json::json;
use std::sync::Arc;

#[derive(Args)]
pub struct ValidateArgs {
    /// Restrict validation to a single dimension type (default: every
    /// type in the configured `dimRelations` chain)
    #[arg(long)]
    pub dim_type: Option<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    args: ValidateArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;
    let inventory: Arc<dyn cubtera_app::InventoryPort> =
        Arc::new(InventoryPortBridge::new(repos.inventory));

    let full_chain: Vec<Ident> = config
        .dim_relations
        .iter()
        .map(|s| Ident::parse(s))
        .collect::<Result<_, _>>()?;

    let graph = load_dim_graph(inventory.as_ref(), &config.org, &full_chain).await?;
    if let Err(errors) = graph.validate() {
        return Err(cubtera_app::AppError::Model(ModelError::Graph(errors)).into());
    }

    let validate_chain = match &args.dim_type {
        Some(t) => vec![Ident::parse(t)?],
        None => full_chain,
    };

    let use_case = ValidateUseCase::new(ResolveUseCase::new(inventory));
    let report = use_case
        .validate_fleet(&graph, &config.org, &validate_chain)
        .await?;

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "valid": report.is_ok(),
                "results": report.results.iter().map(|r| json!({
                    "key": r.key.to_string(),
                    "valid": r.is_ok(),
                    "schema_errors": r.schema_errors,
                    "graph_errors": r.graph_errors,
                })).collect::<Vec<_>>(),
            }))?
        );
    } else if report.is_ok() {
        println!(
            "Valid: {} dimension(s) checked, no violations",
            report.results.len()
        );
    } else {
        eprintln!("Invalid:");
        for failure in report.failures() {
            eprintln!("{failure}");
        }
    }

    if !report.is_ok() {
        std::process::exit(crate::error::EXIT_VALIDATION);
    }

    Ok(())
}
