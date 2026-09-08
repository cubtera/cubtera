//! Inventory management commands
//!
//! v3-native: `cubtera_app::ResolveUseCase` over `FsInventoryPort` (see
//! `run_support::inventory_port`) - no `cubtera-core`/`cubtera-persistence`
//! dependency in this module.

use super::run_support::inventory_port;
use super::Ctx;
use clap::Subcommand;
use cubtera_app::{AppError, ResolveUseCase};
use cubtera_config::Config;
use cubtera_kernel::Ident;
use cubtera_model::Dimension;
use serde_json::{json, Value};

#[derive(Subcommand)]
pub enum ImCommands {
    /// Get all dimension names of a type
    GetAll {
        /// Dimension type (e.g., env, dc)
        dim_type: String,
    },

    /// Get dimension by name
    Get {
        /// Dimension type
        dim_type: String,
        /// Dimension name
        name: String,
    },

    /// Get default dimension for type
    GetDefaults {
        /// Dimension type
        dim_type: String,
    },

    /// Get the JSON-schema for a dimension type (`.schema:meta.json`), if any
    GetSchema {
        /// Dimension type
        dim_type: String,
    },

    /// Get children of a dimension
    GetChildren {
        /// Parent dimension type
        dim_type: String,
        /// Parent dimension name
        name: String,
    },

    /// Get parent of a dimension
    GetParent {
        /// Dimension type
        dim_type: String,
        /// Dimension name
        name: String,
    },

    /// List all dimension types
    GetTypes,

    /// List all organizations
    GetOrgs,

    /// Validate a dimension: exists, and (if the type has a `.schema:meta.json`)
    /// its "meta" section satisfies that schema
    Validate {
        /// Dimension type
        dim_type: String,
        /// Dimension name
        name: String,
    },
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: ImCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let resolve = ResolveUseCase::new(inventory_port(config));

    match cmd {
        ImCommands::GetAll { dim_type } => {
            let names = resolve
                .list_names(&config.org, &parse_ident(&dim_type)?)
                .await?;
            print_list(ctx, &names);
        }

        ImCommands::Get { dim_type, name } => {
            let dim_type = parse_ident(&dim_type)?;
            let name = parse_ident(&name)?;
            let dim = resolve.resolve(&config.org, &dim_type, &name).await?;
            let kids = resolve
                .kids_of(&config.org, &config.dim_relations, &dim_type, &name)
                .await?;
            print_dimension(ctx, &dim, &kids);
        }

        ImCommands::GetDefaults { dim_type } => {
            let dim_type_str = dim_type.clone();
            let dim = resolve
                .get_defaults(&config.org, &parse_ident(&dim_type)?)
                .await?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&dim.as_ref().map(|d| d.to_response_json(&[])))?
                );
            } else {
                match dim {
                    Some(_) => println!("Default for {dim_type_str}: present"),
                    None => println!("No defaults for {dim_type_str}"),
                }
            }
        }

        ImCommands::GetSchema { dim_type } => {
            let dim_type_str = dim_type.clone();
            let schema = resolve
                .get_schema(&config.org, &parse_ident(&dim_type)?)
                .await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&schema)?);
            } else {
                match schema {
                    Some(schema) => println!("{}", serde_json::to_string_pretty(&schema)?),
                    None => println!("No schema for {dim_type_str}"),
                }
            }
        }

        ImCommands::GetChildren { dim_type, name } => {
            let dim_type = parse_ident(&dim_type)?;
            let name = parse_ident(&name)?;
            let children = resolve
                .get_children(&config.org, &config.dim_relations, &dim_type, &name)
                .await?;
            if ctx.json {
                let items: Vec<Value> = children.iter().map(|d| d.to_response_json(&[])).collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                for child in &children {
                    println!("{}", child.key);
                }
            }
        }

        ImCommands::GetParent { dim_type, name } => {
            let dim_type = parse_ident(&dim_type)?;
            let name = parse_ident(&name)?;
            let parent = resolve.get_parent(&config.org, &dim_type, &name).await?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(
                        &parent.as_ref().map(|d| d.to_response_json(&[]))
                    )?
                );
            } else {
                match &parent {
                    Some(parent) => println!("{}", parent.key),
                    None => println!("No parent for {dim_type}:{name}"),
                }
            }
        }

        ImCommands::GetTypes => {
            let types = resolve.list_types(&config.org).await?;
            print_list(ctx, &types);
        }

        ImCommands::GetOrgs => {
            let orgs = resolve.list_orgs().await?;
            print_list(ctx, &orgs);
        }

        ImCommands::Validate { dim_type, name } => {
            let dim_type_i = parse_ident(&dim_type)?;
            let name_i = parse_ident(&name)?;
            let exists = resolve
                .try_resolve(&config.org, &dim_type_i, &name_i)
                .await?
                .is_some();
            if !exists {
                if ctx.json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json!({
                            "valid": false,
                            "exists": false,
                            "errors": [format!("{dim_type}:{name} does not exist")],
                        }))?
                    );
                } else {
                    eprintln!("Invalid: {dim_type}:{name} does not exist");
                }
                std::process::exit(crate::error::EXIT_NOT_FOUND);
            }

            let errors = resolve
                .validate_schema(&config.org, &dim_type_i, &name_i)
                .await?;
            let valid = errors.is_empty();

            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "valid": valid,
                        "exists": true,
                        "errors": errors,
                    }))?
                );
            } else if valid {
                println!("Valid: {dim_type}:{name}");
            } else {
                eprintln!("Invalid: {dim_type}:{name} does not satisfy its schema:");
                for error in &errors {
                    eprintln!("  - {error}");
                }
            }

            if !valid {
                std::process::exit(crate::error::EXIT_VALIDATION);
            }
        }
    }

    Ok(())
}

fn parse_ident(raw: &str) -> Result<Ident, Box<dyn std::error::Error>> {
    Ident::parse(raw).map_err(|e| Box::new(AppError::from(e)) as Box<dyn std::error::Error>)
}

fn print_list(ctx: &Ctx, items: &[String]) {
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(items).unwrap());
    } else {
        for item in items {
            println!("{item}");
        }
    }
}

fn print_dimension(ctx: &Ctx, dim: &Dimension, kids: &[String]) {
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&dim.to_response_json(kids)).unwrap()
        );
        return;
    }

    println!("{}", dim.key);
    if let Some(parent) = &dim.parent_ref {
        println!("  Parent: {parent}");
    }
    if !dim.sections.is_empty() {
        println!("  Data: {} sections", dim.sections.len());
    }
    if !kids.is_empty() {
        println!("  Kids: {kids:?}");
    }
}
