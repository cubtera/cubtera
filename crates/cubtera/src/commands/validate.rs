//! `cubtera validate` - static check of the fleet: every dimension of
//! every configured type against its JSON schema (mandatory in v3, section 5.1)
//! and its dim-graph edge (v2 never checked either at more than one
//! dimension at a time), plus every unit's runner-capability contract
//! (section 5.5: `[outputs] publish = true` under a runner that can't
//! collect outputs is a validation error here, never a silent warning
//! after `apply` - the exact OpenTofu trap the spec calls out). First real
//! command built on `cubtera-app` (P3); see
//! docs/specs/2026-09-03-cubtera-v3-architecture.md section 3/section 10.

use super::Ctx;
use crate::app_bridge::InventoryPortBridge;
use crate::exec_bridge::ExecutorBridge;
use clap::Args;
use cubtera_app::ports::Executor;
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

    let (units_checked, unit_errors) = validate_unit_runner_contracts(config).await?;
    let all_valid = report.is_ok() && unit_errors.is_empty();

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "valid": all_valid,
                "results": report.results.iter().map(|r| json!({
                    "key": r.key.to_string(),
                    "valid": r.is_ok(),
                    "schema_errors": r.schema_errors,
                    "graph_errors": r.graph_errors,
                })).collect::<Vec<_>>(),
                "unit_errors": unit_errors,
            }))?
        );
    } else if all_valid {
        println!(
            "Valid: {} dimension(s) checked, {units_checked} unit(s) checked, no violations",
            report.results.len(),
        );
    } else {
        eprintln!("Invalid:");
        for failure in report.failures() {
            eprintln!("{failure}");
        }
        for e in &unit_errors {
            eprintln!("{e}");
        }
    }

    if !all_valid {
        std::process::exit(crate::error::EXIT_VALIDATION);
    }

    Ok(())
}

/// Every unit whose manifest sets `[outputs] publish = true` must use a
/// runner whose `RunnerCapabilities::collects_outputs` is `true` -
/// otherwise `apply` would silently skip publishing (v2's actual failure
/// mode for a misconfigured OpenTofu/bash unit). Checked statically here
/// instead of discovering it only after a real `apply`. Returns
/// `(units_checked, errors)`.
async fn validate_unit_runner_contracts(
    config: &Config,
) -> Result<(usize, Vec<String>), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;
    let executor = ExecutorBridge::new(std::env::temp_dir(), std::env::temp_dir());

    let unit_names = repos.units.list_units(&config.org).await?;
    let mut errors = Vec::new();
    for unit_name in &unit_names {
        let Some(manifest) = repos.units.find_manifest(&config.org, unit_name).await? else {
            continue;
        };
        if !manifest.publishes_outputs() {
            continue;
        }
        let runner_type = manifest.runner_type().as_str().to_string();
        match executor.capabilities(&runner_type).await {
            Ok(caps) if !caps.collects_outputs => {
                errors.push(format!(
                    "unit '{unit_name}': [outputs] publish = true, but runner '{runner_type}' \
                     does not collect outputs on its own - add [runner] outlet_command to write \
                     cubtera_outputs.json yourself, or set publish = false"
                ));
            }
            Ok(_) => {}
            Err(e) => errors.push(format!(
                "unit '{unit_name}': cannot check runner '{runner_type}' capabilities: {e}"
            )),
        }
    }
    Ok((unit_names.len(), errors))
}
