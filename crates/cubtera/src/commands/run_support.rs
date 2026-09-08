//! Shared plumbing for `cubtera plan`/`cubtera apply`/`cubtera explain run`
//! (v3, P4-run).
//!
//! v2's `commands/run.rs` builds a `Unit` (access policy, dimension data,
//! `[inputs]` resolution) and then hands it straight to `RunService`.
//! Here, building the `Unit` (still v2's `UnitService`, unchanged - the
//! inventory/manifest layer isn't being replaced in P4) is only step one:
//! its resolved dimensions become an `InstanceId`, its materialized files
//! become the real workspace `cubtera-app::RunUseCase` executes against,
//! and its manifest is what tells the CLI which `runner_type` and
//! `[outputs]` settings to hand to the use case. Everything past that -
//! the actual `plan`/`apply` pipeline - lives in `cubtera-app`, not here;
//! this module's only job is gathering the raw inputs it needs.

use crate::app_bridge::InventoryPortBridge;
use crate::exec_bridge::ExecutorBridge;
use cubtera_app::ports::{Executor, SystemClock};
use cubtera_app::{InventoryPort, ResolveUseCase, RunUseCase};
use cubtera_config::Config;
use cubtera_core::ports::Workspace as _;
use cubtera_core::services::{DimensionService, UnitService};
use cubtera_domain::Unit;
use cubtera_kernel::{Digest, Ident, InstanceId};
use cubtera_persistence::fs::FsWorkspace;
use cubtera_persistence::Repositories;
use cubtera_source::FsSource;
use cubtera_store::SqliteStore;
use std::sync::Arc;

/// Everything a `plan`/`apply` command needs after resolving CLI args
/// against the inventory/manifest: the materialized v2 `Unit` (for
/// `runner_type`/`[outputs]`/human-readable printing) plus the
/// `InstanceId`/`RunUseCase` v3's pipeline actually runs against.
pub struct PreparedRun {
    pub unit: Unit,
    pub instance: InstanceId,
    pub use_case: RunUseCase,
}

/// Wire a `RunUseCase` against the real inventory/source/store/executor
/// adapters - the same construction `prepare` needs, but usable on its
/// own for commands that don't resolve a specific unit (`cubtera explain
/// run <run_id>` only needs `Store::list_runs`, not a materialized
/// workspace). `executor_workspace_root` only matters if the command ends
/// up actually executing something; `explain` never does.
pub fn build_use_case(
    config: &Config,
    repos: &Repositories,
    executor_workspace_root: std::path::PathBuf,
) -> Result<RunUseCase, Box<dyn std::error::Error>> {
    let inventory: Arc<dyn InventoryPort> =
        Arc::new(InventoryPortBridge::new(repos.inventory.clone()));
    let resolve = ResolveUseCase::new(inventory);
    let source: Arc<dyn cubtera_source::SourceRepo> = Arc::new(FsSource::new(&config.units_path));
    let store: Arc<dyn cubtera_store::Store> = Arc::new(SqliteStore::open(&config.store_path)?);
    let tf_cache_dir = config
        .store_path
        .parent()
        .map(|p| p.join("tf-cache"))
        .unwrap_or_else(|| std::path::PathBuf::from("tf-cache"));
    let executor: Arc<dyn Executor> =
        Arc::new(ExecutorBridge::new(executor_workspace_root, tf_cache_dir));
    let clock = Arc::new(SystemClock);

    Ok(RunUseCase::new(resolve, source, store, executor, clock))
}

/// Resolve `unit_name`/`dimensions`/`extensions`, materialize the unit's
/// files onto disk (same `FsWorkspace`/`MaterializationPlan` v2's `run`
/// command uses), and wire a `RunUseCase` against the real
/// inventory/source/store/executor adapters, scoped to this unit's
/// materialized workspace.
pub async fn prepare(
    config: &Config,
    unit_name: &str,
    dimensions: &[String],
    extensions: &[String],
) -> Result<PreparedRun, Box<dyn std::error::Error>> {
    let repos = Repositories::from_config(config).await?;
    let hierarchy = Repositories::hierarchy(config);
    let dimensions_service = Arc::new(DimensionService::new(repos.inventory.clone(), hierarchy));
    let unit_service = UnitService::new(repos.units.clone(), dimensions_service);

    let mut unit = unit_service
        .build_unit_with_extensions(&config.org, unit_name, dimensions, extensions)
        .await?;
    let temp_folder = unit.calculate_temp_folder(&config.temp_folder_path);
    unit = unit.with_temp_folder(temp_folder);

    let plan = unit.materialize(&config.modules_path, None)?;
    FsWorkspace::new().apply(&plan).await?;

    let mut dim_refs = Vec::new();
    for dim in &unit.dimensions {
        dim_refs.push(cubtera_kernel::DimRef::parse(&dim.key())?);
    }
    let mut ext_refs = Vec::new();
    for ext in extensions {
        ext_refs.push(cubtera_kernel::DimRef::parse(ext)?);
    }
    let instance = InstanceId::try_new(
        Ident::parse(&config.org)?,
        Ident::parse(unit_name)?,
        dim_refs,
        ext_refs,
    )?;

    let use_case = build_use_case(config, &repos, unit.temp_folder.clone())?;

    Ok(PreparedRun {
        unit,
        instance,
        use_case,
    })
}

/// Hash of the effective, already-merged `Config` in force -
/// `ResolutionManifest::config_digest`'s input. Hashing the whole
/// resolved `Config` (not just a hand-picked subset of fields) means a
/// change to *any* org-level setting - not only the ones P4 happens to
/// read today - shows up as drift between `plan` and `apply`, matching
/// `Digest::of_parts`'s "don't guess which fields matter" rationale
/// elsewhere in the kernel.
pub fn config_digest(config: &Config) -> Result<Digest, Box<dyn std::error::Error>> {
    // `Config::runner`/`state` are `HashMap`s, whose iteration order is
    // randomized per process - hashing `Config` directly would make
    // `config_digest` "drift" on every single invocation, defeating the
    // entire point of pinning it. Sorting every object's keys after
    // converting to `serde_json::Value` fixes this deterministically -
    // note this workspace's `serde_json::Map` is actually `IndexMap`-backed
    // (handlebars enables `preserve_order` transitively), so `to_value`
    // alone is *not* enough; the explicit sort in `canonicalize` below is
    // load-bearing, not defensive.
    let value = canonicalize(serde_json::to_value(config)?);
    let bytes = serde_json::to_vec(&value)?;
    Ok(Digest::of(bytes))
}

/// Recursively sort every object's keys in a `serde_json::Value` tree.
fn canonicalize(value: serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut entries: Vec<(String, serde_json::Value)> =
                map.into_iter().map(|(k, v)| (k, canonicalize(v))).collect();
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            serde_json::Value::Object(entries.into_iter().collect())
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.into_iter().map(canonicalize).collect())
        }
        other => other,
    }
}

/// Default actor identity for `plan`/`apply` when `--actor` isn't given -
/// `$USER`, falling back to a fixed placeholder in environments without
/// one (containers, CI).
pub fn default_actor() -> String {
    std::env::var("USER").unwrap_or_else(|_| "cli".to_string())
}
