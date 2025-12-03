//! Inventory management commands

use clap::Subcommand;
use cubtera_config::Config;
use cubtera_core::services::DimensionService;
use cubtera_persistence::Repositories;
use std::sync::Arc;

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

    /// Validate a dimension exists
    Validate {
        /// Dimension type
        dim_type: String,
        /// Dimension name
        name: String,
    },
}

pub async fn run(config: &Config, cmd: ImCommands) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config)?;
    let service = DimensionService::new(repos.dimensions);

    match cmd {
        ImCommands::GetAll { dim_type } => {
            let names = service.get_all_names(&config.org, &dim_type).await?;
            for name in names {
                println!("{}", name);
            }
        }

        ImCommands::Get { dim_type, name } => {
            let dim = service.get_by_name(&config.org, &dim_type, &name).await?;
            println!("{}:{}", dim.dim_type, dim.name);
            if let Some(parent) = &dim.parent_ref {
                println!("  Parent: {}", parent);
            }
            if !dim.data.is_empty() {
                println!("  Data: {} keys", dim.data.len());
            }
        }

        ImCommands::GetDefaults { dim_type } => {
            if let Some(dim) = service.get_defaults(&config.org, &dim_type).await? {
                println!("Default for {}: {}", dim_type, dim.name);
            } else {
                println!("No defaults for {}", dim_type);
            }
        }

        ImCommands::GetChildren { dim_type, name } => {
            let children = service.get_children(&config.org, &dim_type, &name).await?;
            for child in children {
                println!("{}:{}", child.dim_type, child.name);
            }
        }

        ImCommands::GetParent { dim_type, name } => {
            if let Some(parent) = service.get_parent(&config.org, &dim_type, &name).await? {
                println!("{}:{}", parent.dim_type, parent.name);
            } else {
                println!("No parent for {}:{}", dim_type, name);
            }
        }

        ImCommands::GetTypes => {
            let types = service.get_types(&config.org).await?;
            for t in types {
                println!("{}", t);
            }
        }

        ImCommands::GetOrgs => {
            let orgs = service.get_orgs().await?;
            for org in orgs {
                println!("{}", org);
            }
        }

        ImCommands::Validate { dim_type, name } => {
            let exists = service.validate(&config.org, &dim_type, &name).await?;
            if exists {
                println!("Valid: {}:{}", dim_type, name);
            } else {
                eprintln!("Invalid: {}:{} does not exist", dim_type, name);
                std::process::exit(1);
            }
        }
    }

    Ok(())
}

