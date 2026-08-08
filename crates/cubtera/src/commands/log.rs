//! Log commands

use super::Ctx;
use clap::Subcommand;
use cubtera_config::Config;
use cubtera_persistence::Repositories;
use std::collections::HashMap;

#[derive(Subcommand)]
pub enum LogCommands {
    /// Get deployment logs
    Get {
        /// Query filter (format: key:value), can be specified multiple times.
        /// `unit`/`unit_name`, `command` and `exit_code` match the
        /// corresponding entry fields exactly; anything else (e.g. `env:prod`)
        /// is matched against the dimensions the run was against.
        #[arg(short, long)]
        query: Vec<String>,

        /// Limit number of results (most recent first)
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
}

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    cmd: LogCommands,
) -> Result<(), Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;

    match cmd {
        LogCommands::Get { query, limit } => {
            let query = parse_query(&query)?;
            let entries = repos
                .deployment_log
                .find(&config.org, &query, Some(limit))
                .await?;

            if ctx.json {
                println!("{}", serde_json::to_string_pretty(&entries)?);
            } else if entries.is_empty() {
                println!("No deployment log entries found for org '{}'", config.org);
            } else {
                for entry in &entries {
                    println!(
                        "{}  {:<8}  {}  {}  exit={}  {}ms",
                        format_timestamp(entry.timestamp),
                        entry.command,
                        entry.unit_name,
                        entry.dimensions.join(","),
                        entry.exit_code,
                        entry.duration_ms,
                    );
                }
            }
        }
    }

    Ok(())
}

/// Parse `key:value` CLI filters into a query map. `key:value:with:colons`
/// splits only on the first colon, so values (e.g. an S3 path) can contain
/// colons themselves.
fn parse_query(filters: &[String]) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
    filters
        .iter()
        .map(|filter| match filter.split_once(':') {
            Some((key, value)) => Ok((key.to_string(), value.to_string())),
            None => Err(format!("invalid query filter '{filter}', expected 'key:value'").into()),
        })
        .collect()
}

/// Format a Unix timestamp (UTC) as `YYYY-MM-DD HH:MM:SS` without pulling in
/// a date/time crate for one call site. Uses Howard Hinnant's
/// `civil_from_days` algorithm (public domain), the same one `chrono`'s
/// Gregorian calendar math is built on.
fn format_timestamp(epoch_seconds: i64) -> String {
    let days = epoch_seconds.div_euclid(86_400);
    let secs_of_day = epoch_seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    let hour = secs_of_day / 3600;
    let minute = (secs_of_day % 3600) / 60;
    let second = secs_of_day % 60;
    format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02}")
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { y + 1 } else { y };
    (year, month, day)
}
