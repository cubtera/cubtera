//! Inventory management commands

use super::Ctx;
use clap::Subcommand;
use cubtera_config::Config;
use cubtera_core::services::{DimensionService, SchemaValidation};
use cubtera_domain::Dimension;
use cubtera_persistence::Repositories;
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
    let repos = Repositories::from_config(config).await?;
    let hierarchy = Repositories::hierarchy(config);
    let service = DimensionService::new(repos.inventory, hierarchy);

    match cmd {
        ImCommands::GetAll { dim_type } => {
            let names = service.get_all_names(&config.org, &dim_type).await?;
            print_list(ctx, &names);
        }

        ImCommands::Get { dim_type, name } => {
            let dim = service.get_by_name(&config.org, &dim_type, &name).await?;
            print_dimension(ctx, &dim);
        }

        ImCommands::GetDefaults { dim_type } => {
            let dim = service.get_defaults(&config.org, &dim_type).await?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&dim.as_ref().map(dimension_to_json))?
                );
            } else {
                match dim {
                    Some(dim) => println!("Default for {}: {}", dim_type, dim.name),
                    None => println!("No defaults for {}", dim_type),
                }
            }
        }

        ImCommands::GetSchema { dim_type } => {
            let schema = service.get_schema(&config.org, &dim_type).await?;
            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&schema)?);
            } else {
                match schema {
                    Some(schema) => println!("{}", serde_json::to_string_pretty(&schema)?),
                    None => println!("No schema for {}", dim_type),
                }
            }
        }

        ImCommands::GetChildren { dim_type, name } => {
            let children = service.get_children(&config.org, &dim_type, &name).await?;
            if ctx.json {
                let items: Vec<Value> = children.iter().map(dimension_to_json).collect();
                println!("{}", serde_json::to_string_pretty(&items)?);
            } else {
                for child in &children {
                    println!("{}:{}", child.dim_type, child.name);
                }
            }
        }

        ImCommands::GetParent { dim_type, name } => {
            let parent = service.get_parent(&config.org, &dim_type, &name).await?;
            if ctx.json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&parent.as_ref().map(dimension_to_json))?
                );
            } else {
                match parent {
                    Some(parent) => println!("{}:{}", parent.dim_type, parent.name),
                    None => println!("No parent for {}:{}", dim_type, name),
                }
            }
        }

        ImCommands::GetTypes => {
            let types = service.get_types(&config.org).await?;
            print_list(ctx, &types);
        }

        ImCommands::GetOrgs => {
            let orgs = service.get_orgs().await?;
            print_list(ctx, &orgs);
        }

        ImCommands::Validate { dim_type, name } => {
            let exists = service.validate(&config.org, &dim_type, &name).await?;
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
                    eprintln!("Invalid: {}:{} does not exist", dim_type, name);
                }
                std::process::exit(crate::error::EXIT_NOT_FOUND);
            }

            let outcome = service
                .validate_schema(&config.org, &dim_type, &name)
                .await?;

            if ctx.json {
                let (valid, errors): (bool, Vec<String>) = match &outcome {
                    SchemaValidation::NoSchema | SchemaValidation::Valid => (true, vec![]),
                    SchemaValidation::Invalid(errors) => (false, errors.clone()),
                };
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "valid": valid,
                        "exists": true,
                        "errors": errors,
                    }))?
                );
            } else {
                match &outcome {
                    SchemaValidation::NoSchema => {
                        println!("Valid: {}:{} exists (no schema defined)", dim_type, name)
                    }
                    SchemaValidation::Valid => {
                        println!("Valid: {}:{} satisfies its schema", dim_type, name)
                    }
                    SchemaValidation::Invalid(errors) => {
                        eprintln!(
                            "Invalid: {}:{} does not satisfy its schema:",
                            dim_type, name
                        );
                        for error in errors {
                            eprintln!("  - {error}");
                        }
                    }
                }
            }

            if !outcome.is_ok() {
                std::process::exit(crate::error::EXIT_VALIDATION);
            }
        }
    }

    Ok(())
}

fn print_list(ctx: &Ctx, items: &[String]) {
    if ctx.json {
        println!("{}", serde_json::to_string_pretty(items).unwrap());
    } else {
        for item in items {
            println!("{}", item);
        }
    }
}

fn print_dimension(ctx: &Ctx, dim: &Dimension) {
    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&dimension_to_json(dim)).unwrap()
        );
        return;
    }

    println!("{}:{}", dim.dim_type, dim.name);
    if let Some(parent) = &dim.parent_ref {
        println!("  Parent: {}", parent);
    }
    if !dim.data.is_empty() {
        println!("  Data: {} keys", dim.data.len());
    }
    if !dim.kids.is_empty() {
        println!("  Kids: {:?}", dim.kids);
    }
}

/// Render a [`Dimension`] as one JSON object: its sections plus the
/// resolution metadata (`name`/`type`/`parent`/`key_path`/`data_sha`/`kids`)
/// that isn't itself part of any section's data.
fn dimension_to_json(dim: &Dimension) -> Value {
    let mut obj = match dim.to_json() {
        Value::Object(map) => map,
        _ => serde_json::Map::new(),
    };
    obj.insert("name".to_string(), json!(dim.name));
    obj.insert("type".to_string(), json!(dim.dim_type.as_str()));
    obj.insert("parent".to_string(), json!(dim.parent_ref));
    obj.insert("key_path".to_string(), json!(dim.key_path));
    obj.insert("data_sha".to_string(), json!(dim.data_sha));
    obj.insert("kids".to_string(), json!(dim.kids));
    Value::Object(obj)
}
