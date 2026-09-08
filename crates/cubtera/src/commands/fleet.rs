//! `cubtera fleet ls` - list every resolvable dimension across the
//! configured `dimRelations` chain through the v3 `resolve` pipeline
//! (`cubtera-app`, P3): each entry's resolved `key_path` and content hash,
//! not just its bare name (`cubtera im get-all` gives you the latter).
//!
//! `Binding`/selectors (`cubtera fleet status -s "..."`, P5) don't exist
//! yet, so this is deliberately the "cheapest value" slice the plan calls
//! for: visibility into what the inventory resolves to, with zero new
//! inventory format changes.

use super::Ctx;
use crate::app_bridge::InventoryPortBridge;
use clap::{Args, Subcommand};
use cubtera_app::ResolveUseCase;
use cubtera_config::Config;
use cubtera_kernel::Ident;
use cubtera_persistence::Repositories;
use serde_json::json;
use std::sync::Arc;

#[derive(Subcommand)]
pub enum FleetCommands {
    /// List every dimension resolvable through the configured
    /// `dimRelations` chain
    Ls(FleetLsArgs),
}

#[derive(Args)]
pub struct FleetLsArgs {
    /// Restrict the listing to a single dimension type (default: every
    /// type in the configured `dimRelations` chain)
    #[arg(long)]
    pub dim_type: Option<String>,
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: FleetCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;
    let inventory: Arc<dyn cubtera_app::InventoryPort> =
        Arc::new(InventoryPortBridge::new(repos.inventory));
    let resolve = ResolveUseCase::new(inventory);

    match cmd {
        FleetCommands::Ls(args) => {
            let chain: Vec<Ident> = match &args.dim_type {
                Some(t) => vec![Ident::parse(t)?],
                None => config
                    .dim_relations
                    .iter()
                    .map(|s| Ident::parse(s))
                    .collect::<Result<_, _>>()?,
            };

            let mut entries = Vec::new();
            for dim_type in &chain {
                for name in resolve.list_names(&config.org, dim_type).await? {
                    let name = Ident::parse(&name)?;
                    if let Some(dim) = resolve.try_resolve(&config.org, dim_type, &name).await? {
                        entries.push(dim);
                    }
                }
            }

            if ctx.json {
                let items: Vec<_> = entries
                    .iter()
                    .map(|dim| {
                        json!({
                            "key": dim.key.to_string(),
                            "key_path": dim.key_path.iter().map(ToString::to_string).collect::<Vec<_>>(),
                            "parent": dim.parent_ref.as_ref().map(ToString::to_string),
                            "content_hash": dim.content_hash.to_hex(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else if entries.is_empty() {
                println!("No dimensions resolved for org '{}'", config.org);
            } else {
                for dim in &entries {
                    let path = dim
                        .key_path
                        .iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(" -> ");
                    println!("{}  [{}]  {}", dim.key, path, dim.content_hash.to_hex());
                }
            }
        }
    }

    Ok(())
}
