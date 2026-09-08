//! Shared plumbing for `cubtera plan`/`cubtera apply`/`cubtera explain run`
//! (v3, P4-run/P7-rewire).
//!
//! Every port here is v3-native (`cubtera_inventory::{FsInventoryPort,
//! FsUnitPort}`, `cubtera_app::AssembleUseCase`,
//! `cubtera_exec::apply_materialization_plan`) - no `cubtera-core`/
//! `cubtera-persistence`/`cubtera-domain` dependency left in this module.
//! `AssembleUseCase::build_unit_with_extensions` replaces v2's
//! `UnitService::build_unit_with_extensions` (dimension resolution +
//! `AccessPolicy` + includes aggregation), and
//! `cubtera_exec::apply_materialization_plan` replaces v2's
//! `FsWorkspace::apply` - both proven at parity against v2's behavior by
//! their own unit/golden tests (see `crates/cubtera-app/src/assemble.rs`,
//! `crates/cubtera-inventory/tests/golden_fixture.rs`).
//!
//! The resolved unit's manifest is what tells the CLI which `runner_type`
//! and `[outputs]` settings to hand to `cubtera-app::RunUseCase` - the
//! actual `plan`/`apply` pipeline lives there, not here; this module's job
//! is gathering the raw inputs it needs.

use crate::exec_bridge::ExecutorBridge;
use cubtera_app::ports::{Executor, IdentityProvider, SystemClock};
use cubtera_app::{
    AssembleUseCase, BindingUseCase, InputRequest, InventoryPort, ResolveUseCase, RunUseCase,
    UnitPort,
};
use cubtera_config::Config;
use cubtera_identity::EnvIdentityProvider;
use cubtera_inventory::{FsInventoryPort, FsUnitPort};
use cubtera_kernel::{Digest, DimRef, Ident, InstanceId};
use cubtera_model::{project_state_key, Unit};
use cubtera_source::FsSource;
use cubtera_store::SqliteStore;
use std::sync::Arc;

/// Everything a `plan`/`apply` command needs after resolving CLI args
/// against the inventory/manifest: the materialized v3 `Unit` (for
/// `runner_type`/`[outputs]`/human-readable printing) plus the
/// `InstanceId`/`RunUseCase` the pipeline actually runs against.
pub struct PreparedRun {
    pub unit: Unit,
    pub instance: InstanceId,
    pub use_case: RunUseCase,
}

/// The FS-backed `InventoryPort`, rooted at `config.inventory_path` - the
/// same construction v2's `Repositories::from_config` used for
/// `FsInventoryRepository`, just without a `cubtera-persistence`
/// dependency in between.
pub fn inventory_port(config: &Config) -> Arc<dyn InventoryPort> {
    Arc::new(
        FsInventoryPort::new(config.inventory_path.clone())
            .with_separator(config.file_name_separator.clone()),
    )
}

/// The FS-backed `UnitPort`, rooted at `config.units_path`.
pub fn unit_port(config: &Config) -> Arc<dyn UnitPort> {
    Arc::new(FsUnitPort::new(config.units_path.clone()))
}

/// Wire a `RunUseCase` against the real inventory/source/store/executor
/// adapters - the same construction `prepare` needs, but usable on its
/// own for commands that don't resolve a specific unit (`cubtera explain
/// run <run_id>` only needs `Store::list_runs`, not a materialized
/// workspace). `executor_workspace_root` only matters if the command ends
/// up actually executing something; `explain` never does.
pub fn build_use_case(
    config: &Config,
    executor_workspace_root: std::path::PathBuf,
) -> Result<RunUseCase, Box<dyn std::error::Error>> {
    let resolve = ResolveUseCase::new(inventory_port(config));
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
    let identity: Arc<dyn IdentityProvider> = Arc::new(EnvIdentityProvider);

    Ok(RunUseCase::new(
        resolve, source, store, executor, clock, identity,
    ))
}

/// Build the `InputRequest`s a `plan`/`apply` call should carry for
/// `unit`'s `[inputs.<alias>]` entries - the v3-side equivalent of
/// `UnitService::resolve_inputs` (v2, `crates/cubtera-core/src/services/unit.rs`),
/// projecting each producer's required dimensions onto `unit`'s own
/// resolved chain when the manifest doesn't name them explicitly. Returns
/// `vec![]` for a manifest with no `[inputs]` at all.
pub async fn build_input_requests(
    config: &Config,
    unit: &Unit,
) -> Result<Vec<InputRequest>, Box<dyn std::error::Error>> {
    let org = Ident::parse(&config.org)?;
    let units = unit_port(config);
    let mut requests = Vec::with_capacity(unit.manifest.inputs.len());

    for (alias, spec) in &unit.manifest.inputs {
        let dims = match &spec.dims {
            Some(explicit) => explicit.clone(),
            None => {
                let producer_manifest = units
                    .find_manifest(&config.org, &spec.unit)
                    .await?
                    .ok_or_else(|| format!("unit '{}' not found", spec.unit))?;
                project_state_key(&unit.dim_key_path, &producer_manifest.dimensions)?
            }
        };
        let mut dim_refs = Vec::with_capacity(dims.len());
        for d in &dims {
            dim_refs.push(DimRef::parse(d)?);
        }
        let mut ext_refs = Vec::new();
        for e in spec.ext.iter().flatten() {
            ext_refs.push(DimRef::parse(e)?);
        }

        let producer =
            InstanceId::try_new(org.clone(), Ident::parse(&spec.unit)?, dim_refs, ext_refs)?;
        let expects = spec
            .expects
            .as_deref()
            .map(semver::VersionReq::parse)
            .transpose()?;

        requests.push(InputRequest {
            alias: alias.clone(),
            producer,
            expects,
            required: spec.is_required(),
        });
    }

    Ok(requests)
}

/// Wire a `BindingUseCase` against the real inventory/source/store
/// adapters (P5, `cubtera fleet status`/`cubtera drift`) - no `Executor`
/// needed, since expanding/diffing a `Binding` never runs anything.
/// `leaf_dim_type` is the last entry of `config.dim_relations` (e.g. `dc`):
/// a `Binding`'s selector is evaluated once per name of that type, walking
/// each one's resolved ancestor chain, the same shape `prepare` builds an
/// `InstanceId` from for an ad hoc `plan`/`apply`.
pub fn build_binding_use_case(
    config: &Config,
) -> Result<BindingUseCase, Box<dyn std::error::Error>> {
    let resolve = ResolveUseCase::new(inventory_port(config));
    let source: Arc<dyn cubtera_source::SourceRepo> = Arc::new(FsSource::new(&config.units_path));
    let store: Arc<dyn cubtera_store::Store> = Arc::new(SqliteStore::open(&config.store_path)?);
    let leaf_dim_type = config
        .dim_relations
        .last()
        .ok_or("config.dim_relations is empty - cannot determine the leaf dimension type")?;
    Ok(BindingUseCase::new(
        resolve,
        source,
        store,
        Ident::parse(leaf_dim_type)?,
    ))
}

/// Resolve `unit_name`/`dimensions`/`extensions` through `AssembleUseCase`
/// (dimension resolution + `AccessPolicy` + includes) and assign its temp
/// folder - everything `plan`/`apply`/`explain`/`run --dry-run` need
/// *before* deciding whether to actually touch disk. Split out from
/// [`prepare`] so `cubtera run --dry-run` can print the
/// [`cubtera_model::MaterializationPlan`] without ever calling
/// [`cubtera_exec::apply_materialization_plan`] - the same "resolve first,
/// materialize only if not dry-run" split v2's `run.rs` had.
pub async fn build_unit(
    config: &Config,
    unit_name: &str,
    dimensions: &[String],
    extensions: &[String],
) -> Result<Unit, Box<dyn std::error::Error>> {
    let assemble = AssembleUseCase::new(inventory_port(config), unit_port(config));

    let unit = assemble
        .build_unit_with_extensions(&config.org, unit_name, dimensions, extensions)
        .await?;
    let temp_folder = unit.calculate_temp_folder(&config.temp_folder_path);
    Ok(unit.with_temp_folder(temp_folder))
}

/// [`build_unit`], then resolve any `[inputs.<alias>]` the unit declares
/// against `cubtera-store` (so `cubtera_in_<alias>.json`/
/// `cubtera_inputs.json` land on disk - the same file-based shape v2's
/// consumers relied on, in addition to the `CUBTERA_IN_<ALIAS>`/
/// `TF_VAR_<alias>` env vars `RunUseCase::build_resolution` injects at
/// execute time), materialize the unit's files onto disk
/// (`cubtera_exec::apply_materialization_plan`, the v3-native equivalent
/// of v2's `FsWorkspace::apply`), and wire a `RunUseCase` against the real
/// inventory/source/store/executor adapters, scoped to this unit's
/// materialized workspace.
pub async fn prepare(
    config: &Config,
    unit_name: &str,
    dimensions: &[String],
    extensions: &[String],
) -> Result<PreparedRun, Box<dyn std::error::Error>> {
    let mut unit = build_unit(config, unit_name, dimensions, extensions).await?;

    let mut dim_refs = Vec::new();
    for dim in &unit.dimensions {
        dim_refs.push(dim.clone());
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

    let use_case = build_use_case(config, unit.temp_folder.clone())?;

    let inputs = build_input_requests(config, &unit).await?;
    if !inputs.is_empty() {
        let resolved_inputs = use_case.resolve_inputs_for_materialization(&inputs).await?;
        unit = unit.with_resolved_inputs(resolved_inputs);
    }

    let plan = unit.materialize(&config.modules_path, None)?;
    cubtera_exec::apply_materialization_plan(&plan).await?;

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
