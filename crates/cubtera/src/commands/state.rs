//! Unit state (cross-unit `[outputs]`/`[inputs]`) commands
//!
//! Read/write access to whatever a producer unit last published via
//! `[outputs] publish = true` - independent of any consumer's `[inputs]`
//! resolution, which happens automatically inside `cubtera run`. These
//! commands are for inspecting/clearing published state directly, e.g. to
//! debug a stale value or force a producer to be re-run.
//!
//! v3-native: reads/writes `cubtera_store::LegacyUnitStateRow` (the same
//! SQLite table v2's `UnitStateRepository`/`cubtera run` write to) directly,
//! with no `cubtera-core`/`cubtera-persistence`/`cubtera-domain` dependency
//! in this module. `org`/`unit`/every `dims`/`ext` entry are validated
//! through `cubtera_kernel::{Ident, DimRef}` before touching the store,
//! same rationale as `cubtera_domain::UnitStateKey::try_new` (an
//! unvalidated `dims = ["../../../tmp/pwned"]` would otherwise be a
//! path-traversal payload once a future non-SQL adapter joins it onto a
//! filesystem path).

use super::Ctx;
use clap::Subcommand;
use cubtera_app::AppError;
use cubtera_config::Config;
use cubtera_kernel::{DimRef, Ident};
use cubtera_store::{LegacyUnitStateRow, SqliteStore, Store as _};

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

/// Validate `org`/`unit`/`dims`/`ext` and build the canonical state-key
/// string used to look up (or delete) a [`LegacyUnitStateRow`].
fn state_key(
    org: &str,
    unit: &str,
    dimensions: &[String],
    extensions: &[String],
) -> Result<String, Box<dyn std::error::Error>> {
    Ident::parse(org).map_err(AppError::from)?;
    Ident::parse(unit).map_err(AppError::from)?;
    let dims: Vec<String> = dimensions
        .iter()
        .map(|d| DimRef::parse(d).map(|r| r.key()).map_err(AppError::from))
        .collect::<Result<_, _>>()?;
    let ext: Vec<String> = extensions
        .iter()
        .map(|e| DimRef::parse(e).map(|r| r.key()).map_err(AppError::from))
        .collect::<Result<_, _>>()?;
    Ok(LegacyUnitStateRow::state_key(org, unit, &dims, &ext))
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: StateCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let store = SqliteStore::open(&config.store_path)?;

    match cmd {
        StateCommands::Get {
            unit,
            dimensions,
            extensions,
        } => {
            let key = state_key(&config.org, &unit, &dimensions, &extensions)?;
            let record = store
                .get_legacy_unit_state(&key)
                .await?
                .ok_or_else(|| AppError::not_found("unit state", key.clone()))?;

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
            Ident::parse(&config.org)?;
            Ident::parse(&unit)?;
            let records = store.list_legacy_unit_state(&config.org, &unit).await?;

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
            let key = state_key(&config.org, &unit, &dimensions, &extensions)?;
            store.delete_legacy_unit_state(&key).await?;

            if ctx.json {
                println!("{}", serde_json::json!({"deleted": key}));
            } else {
                println!("Deleted state for {key}");
            }
        }
    }

    Ok(())
}
