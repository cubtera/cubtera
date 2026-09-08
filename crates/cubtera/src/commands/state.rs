//! Unit state (cross-unit `[outputs]`/`[inputs]`) commands
//!
//! Read/write access to whatever a producer unit last published via
//! `[outputs] publish = true` - independent of any consumer's `[inputs]`
//! resolution, which happens automatically inside `cubtera run`. These
//! commands are for inspecting/clearing published state directly, e.g. to
//! debug a stale value or force a producer to be re-run.

use super::Ctx;
use clap::Subcommand;
use cubtera_config::Config;
use cubtera_core::error::AppError;
use cubtera_domain::UnitStateKey;
use cubtera_persistence::Repositories;
use cubtera_store::Store as _;

#[derive(Subcommand)]
pub enum StateCommands {
    /// Get a producer unit's published outputs for an exact dims/ext key
    Get {
        /// Producer unit name
        #[arg(short, long)]
        unit: String,

        /// Dimension (format: type:name) the producer ran with, can be
        /// specified multiple times - must match exactly what it published,
        /// not the consumer's full ancestor chain
        #[arg(short, long = "dim")]
        dimensions: Vec<String>,

        /// Extension (format: type:name) the producer ran with, can be
        /// specified multiple times
        #[arg(short = 'e', long = "ext")]
        extensions: Vec<String>,
    },

    /// List every dims/ext combination a unit has ever published, or (with
    /// `--stale`) every consumer whose recorded `[inputs]` revision is
    /// behind the producer's latest published revision - v3 state-mesh
    /// (P6) visibility into `Store::mark_consumed`/`list_stale_consumers`,
    /// independent of `unit`.
    Ls {
        /// Producer unit name - ignored when `--stale` is set
        #[arg(short, long, required_unless_present = "stale")]
        unit: Option<String>,

        /// List consumers that haven't re-applied since the producer they
        /// depend on last published a newer output revision, across every
        /// unit in `config.org` - not scoped to a single producer.
        #[arg(long)]
        stale: bool,
    },

    /// Remove a producer unit's published outputs for an exact dims/ext key
    Rm {
        /// Producer unit name
        #[arg(short, long)]
        unit: String,

        /// Dimension (format: type:name) the producer ran with, can be
        /// specified multiple times
        #[arg(short, long = "dim")]
        dimensions: Vec<String>,

        /// Extension (format: type:name) the producer ran with, can be
        /// specified multiple times
        #[arg(short = 'e', long = "ext")]
        extensions: Vec<String>,
    },
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: StateCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;

    match cmd {
        StateCommands::Get {
            unit,
            dimensions,
            extensions,
        } => {
            let key = UnitStateKey::try_new(&config.org, &unit, dimensions, extensions)?;
            let record = repos
                .unit_state
                .get(&key)
                .await?
                .ok_or_else(|| AppError::not_found("unit state", key.canonical()))?;

            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&record)?);
            } else {
                println!("unit:       {}", record.unit);
                println!("dims:       {}", record.dims.join(","));
                if !record.ext.is_empty() {
                    println!("ext:        {}", record.ext.join(","));
                }
                println!("updated_at: {}", record.updated_at);
                println!("outputs:");
                println!("{}", serde_json::to_string_pretty(&record.outputs)?);
            }
        }

        StateCommands::Ls { unit: _, stale } if stale => {
            let org = cubtera_kernel::Ident::parse(&config.org)?;
            let store = cubtera_store::SqliteStore::open(&config.store_path)?;
            let stale_consumers = store.list_stale_consumers(&org).await?;

            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&stale_consumers)?);
            } else if stale_consumers.is_empty() {
                println!("No stale consumers - every [inputs] consumer is up to date");
            } else {
                for entry in &stale_consumers {
                    println!(
                        "{}  consumes {} @ rev {} (latest: {})",
                        entry.consumer.canonical(),
                        entry.producer.canonical(),
                        entry.consumed_revision,
                        entry.current_revision,
                    );
                }
            }
        }

        StateCommands::Ls { unit, .. } => {
            let unit = unit.ok_or("--unit is required unless --stale is set")?;
            // `list` uses `org`/`unit` as raw SQL query parameters with no
            // further checks - validate here, same as `Get`/`Rm`'s
            // `UnitStateKey::try_new`.
            cubtera_kernel::Ident::parse(&config.org)?;
            cubtera_kernel::Ident::parse(&unit)?;
            let records = repos.unit_state.list(&config.org, &unit).await?;

            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&records)?);
            } else if records.is_empty() {
                println!("No published state found for unit '{unit}'");
            } else {
                for record in &records {
                    let ext = if record.ext.is_empty() {
                        String::new()
                    } else {
                        format!(" #{}", record.ext.join(","))
                    };
                    println!(
                        "{}  {}{}  updated_at={}",
                        record.unit,
                        record.dims.join(","),
                        ext,
                        record.updated_at,
                    );
                }
            }
        }

        StateCommands::Rm {
            unit,
            dimensions,
            extensions,
        } => {
            let key = UnitStateKey::try_new(&config.org, &unit, dimensions, extensions)?;
            repos.unit_state.delete(&key).await?;

            if ctx.json {
                println!("{}", serde_json::json!({"deleted": key.canonical()}));
            } else {
                println!("Deleted state for {}", key.canonical());
            }
        }
    }

    Ok(())
}
