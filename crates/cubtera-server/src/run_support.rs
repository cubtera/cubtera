//! Shared plumbing for the server's plan/apply/explain/state routes -
//! server-side counterpart to `crates/cubtera/src/commands/run_support.rs`
//! (see its doc comment for the overall shape: `cubtera_app::AssembleUseCase`
//! resolves/materializes the unit, `cubtera_app::RunUseCase` runs it - both
//! v3-native, no `cubtera-core`/`cubtera-persistence`/`cubtera-domain`
//! dependency here). The one real difference from the CLI's version: every
//! function here takes `org` as an explicit parameter instead of reading a
//! single `Config::org` - the server is multi-org the same way
//! `cubtera-api`'s routes already are (`InventoryRepository` never assumed
//! a single org either).

use crate::exec_bridge::ServerExecutor;
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

pub struct PreparedRun {
    pub unit: Unit,
    pub instance: InstanceId,
    pub use_case: RunUseCase,
}

pub fn inventory_port(config: &Config) -> Arc<dyn InventoryPort> {
    Arc::new(
        FsInventoryPort::new(config.inventory_path.clone())
            .with_separator(config.file_name_separator.clone()),
    )
}

pub fn unit_port(config: &Config) -> Arc<dyn UnitPort> {
    Arc::new(FsUnitPort::new(config.units_path.clone()))
}

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
        Arc::new(ServerExecutor::new(executor_workspace_root, tf_cache_dir));
    let clock = Arc::new(SystemClock);
    let identity: Arc<dyn IdentityProvider> = Arc::new(EnvIdentityProvider);

    Ok(RunUseCase::new(
        resolve, source, store, executor, clock, identity,
    ))
}

/// The v3-native equivalent of v2's `UnitService::resolve_inputs` - see
/// the CLI's `run_support::build_input_requests` doc comment for the
/// projection logic. `org` is the request-path org, not `config.org`.
pub async fn build_input_requests(
    config: &Config,
    org: &str,
    unit: &Unit,
) -> Result<Vec<InputRequest>, Box<dyn std::error::Error>> {
    let org_ident = Ident::parse(org)?;
    let units = unit_port(config);
    let mut requests = Vec::with_capacity(unit.manifest.inputs.len());

    for (alias, spec) in &unit.manifest.inputs {
        let dims = match &spec.dims {
            Some(explicit) => explicit.clone(),
            None => {
                let producer_manifest = units
                    .find_manifest(org, &spec.unit)
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

        let producer = InstanceId::try_new(
            org_ident.clone(),
            Ident::parse(&spec.unit)?,
            dim_refs,
            ext_refs,
        )?;
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

/// Resolve/materialize `unit_name` for `org` and wire a `RunUseCase`
/// against it. `org` comes from the request path, not `config.org` (which
/// only sets the CLI's/`cubtera-mcp`'s default and is otherwise unused
/// here).
pub async fn prepare(
    config: &Config,
    org: &str,
    unit_name: &str,
    dimensions: &[String],
    extensions: &[String],
) -> Result<PreparedRun, Box<dyn std::error::Error>> {
    let assemble = AssembleUseCase::new(inventory_port(config), unit_port(config));

    let mut unit = assemble
        .build_unit_with_extensions(org, unit_name, dimensions, extensions)
        .await?;
    let temp_folder = unit.calculate_temp_folder(&config.temp_folder_path);
    unit = unit.with_temp_folder(temp_folder);

    let mut dim_refs: Vec<DimRef> = Vec::new();
    for dim in &unit.dimensions {
        dim_refs.push(dim.clone());
    }
    let mut ext_refs = Vec::new();
    for ext in extensions {
        ext_refs.push(cubtera_kernel::DimRef::parse(ext)?);
    }
    let instance = InstanceId::try_new(
        Ident::parse(org)?,
        Ident::parse(unit_name)?,
        dim_refs,
        ext_refs,
    )?;

    let use_case = build_use_case(config, unit.temp_folder.clone())?;

    // Resolve `[inputs.<alias>]` against `cubtera-store` before
    // materializing, so `cubtera_in_<alias>.json`/`cubtera_inputs.json`
    // land on disk (the file-based shape v2's consumers relied on) - see
    // the CLI's `run_support::prepare` doc comment for the rationale.
    let inputs = build_input_requests(config, org, &unit).await?;
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

pub fn config_digest(config: &Config) -> Result<Digest, Box<dyn std::error::Error>> {
    let value = canonicalize(serde_json::to_value(config)?);
    let bytes = serde_json::to_vec(&value)?;
    Ok(Digest::of(bytes))
}

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
