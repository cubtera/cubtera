//! `cubtera migrate` - v2 -> v3 config/data migration helper (v3, P7).
//!
//! v3 replaced v2's three-way deployment-log/unit-state backend choice
//! (fs-jsonl, fs-json, MongoDB) with a single SQLite `Store`
//! (`cubtera_store::SqliteStore`, `config.store_path`) - see
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9/ยง10. An existing
//! v1.x/v2 install lands here with:
//!
//! - a `config.toml` that may still carry now-dead `deploymentLogPath`/
//!   `[deploymentLog]`/`unitStatePath`/`[unitState]` keys per org table
//!   (harmless - `cubtera_config`'s loader silently ignores unknown keys -
//!   but worth cleaning up so the file on disk reflects what's actually in
//!   effect, and so nobody keeps editing a `[deploymentLog]` block that
//!   does nothing);
//! - historical deployment-log entries under
//!   `{deploymentLogPath}/{org}.jsonl` (one JSON object per line, the v2
//!   `FsDeploymentLogRepository` layout);
//! - historical unit-state records under
//!   `{unitStatePath}/{org}/{unit}/{dims...}/{ext...}/outputs.json` (the
//!   v2 `FsUnitStateRepository` layout).
//!
//! This command imports both data sources into `config.store_path`'s
//! SQLite file, into the exact same `legacy_deployment_log`/
//! `legacy_unit_state` tables `cubtera log`/`cubtera state` already read
//! (see `cubtera_store::{LegacyDeploymentLogRow, LegacyUnitStateRow}`), and
//! rewrites `config.toml` to drop the now-dead keys and make `storePath`
//! explicit. Inventory/units/modules on-disk layout never changes - that's
//! the one v1 contract that was never allowed to break (`AGENTS.md`'s
//! "Inventory on-disk format") - so nothing there needs touching.
//!
//! Dry-run by default (reports what it would do, touches nothing); pass
//! `--apply` to actually write. Unit-state import is a plain upsert
//! (`Store::put_legacy_unit_state`), so re-running with the same source
//! directory is idempotent; deployment-log import
//! (`Store::append_legacy_deployment_log`) is append-only like the v2
//! backend it replaces, so re-running against a store that already has
//! that data will duplicate rows - migrate once per store.

use super::Ctx;
use cubtera_config::Config;
use cubtera_store::{LegacyDeploymentLogRow, LegacyUnitStateRow, SqliteStore};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(clap::Args)]
pub struct MigrateArgs {
    /// Base directory of a v2 fs-jsonl deployment log
    /// (`{path}/{org}.jsonl`) to import into `config.store_path`.
    #[arg(long)]
    pub dlog_path: Option<PathBuf>,

    /// Base directory of a v2 fs-json unit-state tree
    /// (`{path}/{org}/{unit}/{dims...}/{ext...}/outputs.json`) to import
    /// into `config.store_path`.
    #[arg(long)]
    pub unit_state_path: Option<PathBuf>,

    /// Actually write changes. Without this flag, `migrate` only reports
    /// what it *would* do - no store writes, no `config.toml` edit.
    #[arg(long)]
    pub apply: bool,
}

/// Keys (and, for the two MongoDB tables, whole sections) that no longer
/// map onto anything in v3 - see the module doc comment. Checked against
/// the raw parsed TOML (not `cubtera_config::Config`, which already
/// silently drops these) so `migrate` can tell the operator exactly what
/// it found and would remove.
const DEAD_SCALAR_KEYS: &[&str] = &["deploymentLogPath", "unitStatePath"];
const DEAD_TABLE_KEYS: &[&str] = &["deploymentLog", "unitState"];

pub async fn run(
    config: &Config,
    ctx: &Ctx,
    config_path: &Path,
    args: MigrateArgs,
) -> Result<(), Box<dyn std::error::Error>> {
    let mode = if args.apply { "apply" } else { "dry-run" };
    println!("cubtera migrate ({mode})");
    println!();

    let config_report = plan_config_cleanup(config_path)?;
    print_config_report(&config_report, config);

    let dlog_report = match &args.dlog_path {
        Some(base) => Some(plan_dlog_import(base, &config.org)?),
        None => None,
    };
    print_dlog_report(dlog_report.as_ref(), &args);

    let unit_state_report = match &args.unit_state_path {
        Some(base) => Some(plan_unit_state_import(base, &config.org)?),
        None => None,
    };
    print_unit_state_report(unit_state_report.as_ref(), &args);

    if !args.apply {
        println!();
        println!("Nothing written (dry-run). Re-run with --apply to perform the migration.");
        return Ok(());
    }

    let store = SqliteStore::open(&config.store_path)?;
    let mut imported_dlog = 0usize;
    let mut imported_unit_state = 0usize;

    if let Some(report) = &dlog_report {
        for row in &report.rows {
            store.append_legacy_deployment_log(row.clone()).await?;
            imported_dlog += 1;
        }
    }

    if let Some(report) = &unit_state_report {
        for (key, row) in &report.rows {
            store
                .put_legacy_unit_state(key.clone(), row.clone())
                .await?;
            imported_unit_state += 1;
        }
    }

    if config_report.has_dead_keys() {
        rewrite_config(config_path, &config_report, config)?;
    }

    println!();
    println!("Done:");
    println!("  deployment log entries imported: {imported_dlog}");
    println!("  unit state records imported:     {imported_unit_state}");
    if config_report.has_dead_keys() {
        println!(
            "  config.toml rewritten (backup at {})",
            config_report.backup_path.display()
        );
    }

    if ctx.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "importedDeploymentLogEntries": imported_dlog,
                "importedUnitStateRecords": imported_unit_state,
                "configRewritten": config_report.has_dead_keys(),
            }))?
        );
    }

    Ok(())
}

// --- config.toml cleanup -------------------------------------------------

struct ConfigCleanupReport {
    path: PathBuf,
    backup_path: PathBuf,
    /// `(org table name, dead key/table name)` pairs found, in file order.
    dead_keys: Vec<(String, String)>,
    raw: toml::Value,
}

impl ConfigCleanupReport {
    fn has_dead_keys(&self) -> bool {
        !self.dead_keys.is_empty()
    }
}

fn plan_config_cleanup(path: &Path) -> Result<ConfigCleanupReport, Box<dyn std::error::Error>> {
    let contents = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ConfigCleanupReport {
                path: path.to_path_buf(),
                backup_path: path.with_extension("toml.bak"),
                dead_keys: Vec::new(),
                raw: toml::Value::Table(Default::default()),
            });
        }
        Err(e) => return Err(format!("failed to read {path:?}: {e}").into()),
    };

    let raw: toml::Value =
        toml::from_str(&contents).map_err(|e| format!("failed to parse {path:?} as TOML: {e}"))?;

    let mut dead_keys = Vec::new();
    if let toml::Value::Table(top) = &raw {
        for (table_name, table_value) in top {
            let toml::Value::Table(table) = table_value else {
                continue;
            };
            for key in DEAD_SCALAR_KEYS {
                if table.contains_key(*key) {
                    dead_keys.push((table_name.clone(), (*key).to_string()));
                }
            }
            for key in DEAD_TABLE_KEYS {
                if table.contains_key(*key) {
                    dead_keys.push((table_name.clone(), (*key).to_string()));
                }
            }
        }
    }

    Ok(ConfigCleanupReport {
        path: path.to_path_buf(),
        backup_path: path.with_extension("toml.bak"),
        dead_keys,
        raw,
    })
}

fn print_config_report(report: &ConfigCleanupReport, config: &Config) {
    println!("config.toml: {}", report.path.display());
    if report.dead_keys.is_empty() {
        println!(
            "  no dead v2 keys found (deploymentLogPath/[deploymentLog]/unitStatePath/[unitState])"
        );
        return;
    }
    for (table, key) in &report.dead_keys {
        println!(
            "  - stale key `{key}` in [{table}] (superseded by storePath = {})",
            config.store_path.display()
        );
    }
    println!("  -> would rewrite config.toml removing these keys (backup kept at *.toml.bak)");
}

fn rewrite_config(
    path: &Path,
    report: &ConfigCleanupReport,
    config: &Config,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut raw = report.raw.clone();
    if let toml::Value::Table(top) = &mut raw {
        for (table, key) in &report.dead_keys {
            if let Some(toml::Value::Table(table_value)) = top.get_mut(table) {
                table_value.remove(key);
            }
        }
        // Make storePath explicit in [default] so the migrated file is
        // self-documenting instead of relying on `cubtera-config`'s
        // built-in default staying in sync with this message.
        let default_table = top
            .entry("default")
            .or_insert_with(|| toml::Value::Table(Default::default()));
        if let toml::Value::Table(default_table) = default_table {
            default_table
                .entry("storePath")
                .or_insert_with(|| toml::Value::String(config.store_path.display().to_string()));
        }
    }

    let original = std::fs::read_to_string(path).unwrap_or_default();
    std::fs::write(&report.backup_path, original)
        .map_err(|e| format!("failed to write backup {:?}: {e}", report.backup_path))?;

    let rendered =
        toml::to_string_pretty(&raw).map_err(|e| format!("failed to render config.toml: {e}"))?;
    std::fs::write(path, rendered).map_err(|e| format!("failed to write {path:?}: {e}"))?;
    Ok(())
}

// --- deployment log import ------------------------------------------------

/// Mirrors v2's `cubtera_core::ports::DeploymentLogEntry` shape exactly -
/// same field names, so an old `{org}.jsonl` line deserializes straight
/// into this without any v2 crate dependency.
#[derive(Debug, Clone, serde::Deserialize)]
struct LegacyDlogEntry {
    unit_name: String,
    org: String,
    dimensions: Vec<String>,
    command: String,
    exit_code: i32,
    timestamp: i64,
    duration_ms: u64,
    #[serde(default)]
    git_shas: HashMap<String, String>,
    #[serde(default)]
    metadata: HashMap<String, serde_json::Value>,
}

impl From<LegacyDlogEntry> for LegacyDeploymentLogRow {
    fn from(e: LegacyDlogEntry) -> Self {
        LegacyDeploymentLogRow {
            org: e.org,
            unit_name: e.unit_name,
            dimensions: e.dimensions,
            command: e.command,
            exit_code: e.exit_code,
            timestamp: e.timestamp,
            duration_ms: e.duration_ms,
            git_shas: e.git_shas.into_iter().collect(),
            metadata: e.metadata.into_iter().collect(),
        }
    }
}

struct DlogImportReport {
    source: PathBuf,
    rows: Vec<LegacyDeploymentLogRow>,
    skipped: Vec<String>,
}

fn plan_dlog_import(
    base: &Path,
    org: &str,
) -> Result<DlogImportReport, Box<dyn std::error::Error>> {
    let source = base.join(format!("{org}.jsonl"));
    let mut rows = Vec::new();
    let mut skipped = Vec::new();

    if let Ok(contents) = std::fs::read_to_string(&source) {
        for (n, line) in contents.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<LegacyDlogEntry>(line) {
                Ok(entry) => rows.push(entry.into()),
                Err(e) => skipped.push(format!("line {}: {e}", n + 1)),
            }
        }
    }

    Ok(DlogImportReport {
        source,
        rows,
        skipped,
    })
}

fn print_dlog_report(report: Option<&DlogImportReport>, args: &MigrateArgs) {
    println!();
    let Some(report) = report else {
        println!("deployment log: --dlog-path not given, skipping");
        return;
    };
    println!("deployment log: {}", report.source.display());
    if !report.source.exists() {
        println!("  file not found, nothing to import");
        return;
    }
    println!(
        "  found {} entr{}",
        report.rows.len(),
        if report.rows.len() == 1 { "y" } else { "ies" }
    );
    for reason in &report.skipped {
        println!("  ! skipped unparseable line ({reason})");
    }
    if !args.apply {
        println!("  -> would import into the SQLite store");
    }
}

// --- unit state import -----------------------------------------------------

/// Mirrors v2's `cubtera_domain::UnitStateRecord` shape exactly.
#[derive(Debug, Clone, serde::Deserialize)]
struct LegacyUnitStateRecord {
    org: String,
    unit: String,
    dims: Vec<String>,
    #[serde(default)]
    ext: Vec<String>,
    outputs: serde_json::Value,
    updated_at: i64,
}

struct UnitStateImportReport {
    source: PathBuf,
    rows: Vec<(String, LegacyUnitStateRow)>,
    skipped: Vec<String>,
}

fn plan_unit_state_import(
    base: &Path,
    org: &str,
) -> Result<UnitStateImportReport, Box<dyn std::error::Error>> {
    let org_dir = base.join(org);
    let mut files = Vec::new();
    if org_dir.exists() {
        walk_outputs_json(&org_dir, &mut files)?;
    }

    let mut rows = Vec::new();
    let mut skipped = Vec::new();
    for path in files {
        let contents = match std::fs::read_to_string(&path) {
            Ok(c) => c,
            Err(e) => {
                skipped.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        match serde_json::from_str::<LegacyUnitStateRecord>(&contents) {
            Ok(record) => {
                let key = LegacyUnitStateRow::state_key(
                    &record.org,
                    &record.unit,
                    &record.dims,
                    &record.ext,
                );
                let row = LegacyUnitStateRow {
                    org: record.org,
                    unit: record.unit,
                    dims: record.dims,
                    ext: record.ext,
                    outputs: record.outputs,
                    updated_at: record.updated_at,
                };
                rows.push((key, row));
            }
            Err(e) => skipped.push(format!("{}: {e}", path.display())),
        }
    }

    Ok(UnitStateImportReport {
        source: org_dir,
        rows,
        skipped,
    })
}

/// Recursively find every `outputs.json` under `dir` - same walk v2's
/// `FsUnitStateRepository::list_paths` did.
fn walk_outputs_json(dir: &Path, found: &mut Vec<PathBuf>) -> std::io::Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            walk_outputs_json(&path, found)?;
        } else if path
            .file_name()
            .map(|n| n == "outputs.json")
            .unwrap_or(false)
        {
            found.push(path);
        }
    }
    Ok(())
}

fn print_unit_state_report(report: Option<&UnitStateImportReport>, args: &MigrateArgs) {
    println!();
    let Some(report) = report else {
        println!("unit state: --unit-state-path not given, skipping");
        return;
    };
    println!("unit state: {}", report.source.display());
    if !report.source.exists() {
        println!("  directory not found, nothing to import");
        return;
    }
    println!(
        "  found {} record{}",
        report.rows.len(),
        if report.rows.len() == 1 { "" } else { "s" }
    );
    for reason in &report.skipped {
        println!("  ! skipped unparseable file ({reason})");
    }
    if !args.apply {
        println!("  -> would import into the SQLite store");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, contents: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }

    #[test]
    fn plan_config_cleanup_finds_dead_scalar_and_table_keys() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write(
            &path,
            r#"
            [default]
            deploymentLogPath = "dlog"
            unitStatePath = "state"

            [cubtera]
            [cubtera.deploymentLog]
            connectionString = "mongodb://localhost:27017"
            "#,
        );

        let report = plan_config_cleanup(&path).unwrap();
        assert!(report.has_dead_keys());
        assert!(report
            .dead_keys
            .contains(&("default".to_string(), "deploymentLogPath".to_string())));
        assert!(report
            .dead_keys
            .contains(&("default".to_string(), "unitStatePath".to_string())));
        assert!(report
            .dead_keys
            .contains(&("cubtera".to_string(), "deploymentLog".to_string())));
    }

    #[test]
    fn plan_config_cleanup_is_clean_for_an_already_migrated_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write(
            &path,
            r#"
            [default]
            storePath = "store.sqlite"
            "#,
        );

        let report = plan_config_cleanup(&path).unwrap();
        assert!(!report.has_dead_keys());
    }

    #[test]
    fn rewrite_config_removes_dead_keys_and_adds_store_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        write(
            &path,
            r#"
            [default]
            deploymentLogPath = "dlog"
            unitStatePath = "state"
            inventoryPath = "inventory"
            "#,
        );

        let report = plan_config_cleanup(&path).unwrap();
        let config = Config {
            store_path: PathBuf::from("/tmp/store.sqlite"),
            ..Config::default()
        };
        rewrite_config(&path, &report, &config).unwrap();

        let rewritten = std::fs::read_to_string(&path).unwrap();
        assert!(!rewritten.contains("deploymentLogPath"));
        assert!(!rewritten.contains("unitStatePath"));
        assert!(rewritten.contains("inventoryPath"));
        assert!(rewritten.contains("store.sqlite"));

        let backup = std::fs::read_to_string(report.backup_path).unwrap();
        assert!(backup.contains("deploymentLogPath"));
    }

    #[test]
    fn plan_dlog_import_parses_jsonl_lines_into_legacy_rows() {
        let dir = tempfile::tempdir().unwrap();
        let dlog_dir = dir.path().join("dlog");
        std::fs::create_dir_all(&dlog_dir).unwrap();
        std::fs::write(
            dlog_dir.join("cubtera.jsonl"),
            r#"{"unit_name":"tf_unit01","org":"cubtera","dimensions":["dome:mgmt"],"command":"apply","exit_code":0,"timestamp":1700000000,"duration_ms":1234,"git_shas":{},"metadata":{}}
{"unit_name":"tf_unit02","org":"cubtera","dimensions":["dc:stg1"],"command":"init","exit_code":1,"timestamp":1700000100,"duration_ms":50,"git_shas":{},"metadata":{}}
"#,
        )
        .unwrap();

        let report = plan_dlog_import(&dlog_dir, "cubtera").unwrap();
        assert_eq!(report.rows.len(), 2);
        assert!(report.skipped.is_empty());
        assert_eq!(report.rows[0].unit_name, "tf_unit01");
        assert_eq!(report.rows[1].exit_code, 1);
    }

    #[test]
    fn plan_dlog_import_reports_unparseable_lines_without_failing() {
        let dir = tempfile::tempdir().unwrap();
        let dlog_dir = dir.path().join("dlog");
        std::fs::create_dir_all(&dlog_dir).unwrap();
        std::fs::write(dlog_dir.join("cubtera.jsonl"), "not json at all\n").unwrap();

        let report = plan_dlog_import(&dlog_dir, "cubtera").unwrap();
        assert!(report.rows.is_empty());
        assert_eq!(report.skipped.len(), 1);
    }

    #[test]
    fn plan_dlog_import_is_empty_when_source_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let report = plan_dlog_import(&dir.path().join("nope"), "cubtera").unwrap();
        assert!(report.rows.is_empty());
        assert!(!report.source.exists());
    }

    #[test]
    fn plan_unit_state_import_walks_nested_outputs_json_files() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("state");
        let record = serde_json::json!({
            "org": "cubtera",
            "unit": "tf_unit02",
            "dims": ["dc:stg1-use2"],
            "ext": [],
            "outputs": {"vpc_id": "vpc-123"},
            "updated_at": 1700000000i64,
        });
        write(
            &base.join("cubtera/tf_unit02/dc:stg1-use2/outputs.json"),
            &record.to_string(),
        );

        let report = plan_unit_state_import(&base, "cubtera").unwrap();
        assert_eq!(report.rows.len(), 1);
        assert!(report.skipped.is_empty());
        let (key, row) = &report.rows[0];
        assert_eq!(row.unit, "tf_unit02");
        assert_eq!(
            key,
            &LegacyUnitStateRow::state_key(
                "cubtera",
                "tf_unit02",
                &["dc:stg1-use2".to_string()],
                &[]
            )
        );
    }

    #[test]
    fn plan_unit_state_import_is_empty_when_org_directory_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let report = plan_unit_state_import(&dir.path().join("state"), "cubtera").unwrap();
        assert!(report.rows.is_empty());
    }
}
