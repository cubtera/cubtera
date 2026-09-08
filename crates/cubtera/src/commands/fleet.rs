//! `cubtera fleet ls` - list every resolvable dimension across the
//! configured `dimRelations` chain through the v3 `resolve` pipeline
//! (`cubtera-app`, P3): each entry's resolved `key_path` and content hash,
//! not just its bare name (`cubtera im get-all` gives you the latter).
//!
//! `Binding`/selectors land in P5 as `cubtera fleet status`: desired state
//! (a unit + a `Selector` expression over the inventory, ad hoc from CLI
//! flags rather than a `bindings/*.toml` file - that loader is a natural
//! follow-up, not part of this slice) diffed against `Store`.

use super::run_support::{build_binding_use_case, inventory_port};
use super::Ctx;
use clap::{Args, Subcommand};
use cubtera_app::{DriftState, ResolveUseCase};
use cubtera_config::Config;
use cubtera_kernel::{DimRef, Ident};
use cubtera_model::{Binding, Selector};
use serde_json::json;

#[derive(Subcommand)]
pub enum FleetCommands {
    /// List every dimension resolvable through the configured
    /// `dimRelations` chain
    Ls(FleetLsArgs),
    /// Expand a `Binding` (unit + selector) and diff it against `Store`:
    /// desired-but-never-applied, up to date, package-drifted, or
    /// orphaned (v3, P5)
    Status(FleetStatusArgs),
}

#[derive(Args)]
pub struct FleetLsArgs {
    /// Restrict the listing to a single dimension type (default: every
    /// type in the configured `dimRelations` chain)
    #[arg(long)]
    pub dim_type: Option<String>,
}

#[derive(Args)]
pub struct FleetStatusArgs {
    /// Unit this binding applies to
    #[arg(short, long)]
    pub unit: String,
    /// Selector expression over the inventory (e.g. `"env.name == 'prod'
    /// && dc.status == 'active'"`) - defaults to matching everything
    #[arg(short, long)]
    pub selector: Option<String>,
    /// Exclude any candidate whose resolved dimensions include this
    /// `type:name` ref (repeatable) - a coarser, CLI-only convenience over
    /// `Binding.exclude`'s exact-`InstanceId` semantics (section 5.4's
    /// `bindings/*.toml` example, `exclude = ["dc:prod-sa1"]`, reads the
    /// same way: "any instance in that dc", not one specific instance)
    #[arg(long = "exclude")]
    pub exclude: Vec<String>,
}

pub fn parse_binding(
    unit: &str,
    selector: Option<&str>,
) -> Result<Binding, Box<dyn std::error::Error>> {
    Ok(Binding {
        id: format!("cli:{unit}"),
        unit: Ident::parse(unit)?,
        selector: match selector {
            Some(s) => Selector::parse(s)?,
            None => Selector::All,
        },
        exclude: Vec::new(),
        wave: 0,
    })
}

/// CLI-only post-filter matching `FleetStatusArgs::exclude`'s coarser
/// semantics - see that field's doc comment.
pub fn excluded_by_dim_ref(id: &cubtera_kernel::InstanceId, exclude: &[DimRef]) -> bool {
    exclude.iter().any(|ex| id.all_refs().any(|r| r == ex))
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: FleetCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolve = ResolveUseCase::new(inventory_port(config));

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
        FleetCommands::Status(args) => {
            let binding = parse_binding(&args.unit, args.selector.as_deref())?;
            let exclude: Vec<DimRef> = args
                .exclude
                .iter()
                .map(|s| DimRef::parse(s))
                .collect::<Result<_, _>>()?;

            let binding_uc = build_binding_use_case(config)?;
            let report = binding_uc.status(&config.org, &binding).await?;
            let report: Vec<_> = report
                .into_iter()
                .filter(|d| !excluded_by_dim_ref(&d.id, &exclude))
                .collect();

            print_drift_report(ctx, &report);
        }
    }

    Ok(())
}

/// Shared human/JSON rendering for `fleet status` and `drift`.
pub fn print_drift_report(ctx: &Ctx, report: &[cubtera_app::InstanceDrift]) {
    if ctx.json {
        let items: Vec<_> = report
            .iter()
            .map(|d| {
                json!({
                    "instance": d.id.canonical(),
                    "state": format!("{:?}", d.state),
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items).unwrap());
    } else if report.is_empty() {
        println!("No matching instances");
    } else {
        for d in report {
            let label = match d.state {
                DriftState::Desired => "desired (never applied)",
                DriftState::UpToDate => "up to date",
                DriftState::PackageDrifted => "PACKAGE DRIFTED",
                DriftState::Orphaned => "ORPHANED",
            };
            println!("{}  {}", d.id.canonical(), label);
        }
    }
}
