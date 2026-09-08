//! `RunUseCase`: `plan` / `apply --plan` / `explain run` (P4-run).
//!
//! This is the pipeline the spec's introduction calls out as *the* v2
//! architecture smell: "building the backend config and `RunParams` lives
//! in `crates/cubtera/src/commands/run.rs`, which is why the API can't
//! physically run units". Here it lives in `cubtera-app`, behind
//! `Executor`/`Store`/`SourceRepo`/`Clock` ports - the CLI (P4) only
//! gathers raw inputs (which dims to resolve, which runner type) and
//! translates `Executor` calls onto `cubtera-exec`'s `RunnerStrategy`, the
//! same bridge shape already used for P3's `InventoryPortBridge`.
//!
//! `plan()` is gated by `Executor::capabilities().supports_plan_artifact`,
//! so a bash unit (no plan concept) gets a clear [`AppError::Validation`]
//! instead of silently running something meaningless. `apply()` requires a
//! previously-created [`Plan`] and refuses to proceed if
//! [`Plan::pins_match`] fails against a freshly recomputed
//! [`ResolutionManifest`], the "approval gate" §7 calls for, something v2
//! had no mechanism for at all. Every apply also takes a `Store` lease on
//! the target [`InstanceId`]; v2 only ever locked `init`, via a TCP port,
//! everything else raced.

use crate::error::{AppError, AppResult};
use crate::ports::{Clock, ExecRequest, Executor, IdentityProvider};
use crate::resolve::ResolveUseCase;
use cubtera_kernel::{Digest, Ident, InstanceId};
use cubtera_model::{
    Instance, OutputSet, OutputValue, Plan, PlanId, ResolutionManifest, Revision, Run, RunFilter,
    RunId, RunOp, RunPatch, RunStatus, UnitPackage,
};
use cubtera_source::SourceRepo;
use cubtera_store::Store;
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// One `[inputs.<alias>]` entry, already resolved to a concrete producer
/// `InstanceId` (projection against the producer's own required
/// dimensions - `cubtera_model::project_state_key` - is the caller's job;
/// see `crates/cubtera/src/commands/run_support.rs`, which already has
/// both the consumer's resolved dim chain and the producer's manifest
/// on hand). State-mesh v2 (section 5.5): `expects` is checked against the
/// producer's `OutputSet::schema_version` before the value is trusted for
/// anything.
#[derive(Debug, Clone)]
pub struct InputRequest {
    pub alias: String,
    pub producer: InstanceId,
    pub expects: Option<semver::VersionReq>,
    pub required: bool,
}

/// What to resolve and run for `cubtera plan`.
pub struct PlanRequest {
    pub instance: InstanceId,
    pub runner_type: String,
    pub command: Vec<String>,
    pub actor: Ident,
    /// Hash of the effective (merged) `config.toml` in force - part of the
    /// pin set, so a config change between `plan` and `apply` is detected.
    pub config_digest: Digest,
    pub ttl_seconds: i64,
    pub inputs: Vec<InputRequest>,
}

/// What to run for `cubtera apply --plan <id>`.
pub struct ApplyRequest {
    pub runner_type: String,
    pub command: Vec<String>,
    pub auto_approve: bool,
    pub actor: Ident,
    pub config_digest: Digest,
    /// Whether this unit's manifest declares `[outputs] publish = true` -
    /// `cubtera-app` doesn't parse `unit.toml` itself (P4 scope), so the
    /// caller (which already loaded the manifest to pick `runner_type`)
    /// supplies this.
    pub publish_outputs: bool,
    pub outputs_schema_version: semver::Version,
    pub lease_ttl: Duration,
    /// Must be the exact same list `plan()` was given for this instance -
    /// `apply` re-resolves inputs itself (so `Plan::pins_match` catches a
    /// producer that moved between `plan` and `apply`, same as package/
    /// inventory/config drift), it doesn't replay `plan`'s resolution.
    pub inputs: Vec<InputRequest>,
}

pub struct RunUseCase {
    resolve: ResolveUseCase,
    source: Arc<dyn SourceRepo>,
    store: Arc<dyn Store>,
    executor: Arc<dyn Executor>,
    clock: Arc<dyn Clock>,
    identity: Arc<dyn IdentityProvider>,
}

impl RunUseCase {
    pub fn new(
        resolve: ResolveUseCase,
        source: Arc<dyn SourceRepo>,
        store: Arc<dyn Store>,
        executor: Arc<dyn Executor>,
        clock: Arc<dyn Clock>,
        identity: Arc<dyn IdentityProvider>,
    ) -> Self {
        Self {
            resolve,
            source,
            store,
            executor,
            clock,
            identity,
        }
    }

    /// Resolve every dimension `instance` names, build the
    /// dimension-derived `variables` a runner should see, and hash the
    /// unit's own package (manifest + files, no pinned modules yet - see
    /// the doc comment at the bottom of this file) - the shared
    /// computation `plan()` and `apply()` must agree on byte-for-byte, or
    /// `Plan::pins_match` would be meaningless.
    async fn build_resolution(
        &self,
        instance: &InstanceId,
        config_digest: Digest,
        inputs: &[InputRequest],
    ) -> AppResult<(ResolutionManifest, BTreeMap<String, Value>)> {
        let org = instance.org().as_str();
        let mut variables: BTreeMap<String, Value> = BTreeMap::new();
        let mut inventory_digests: BTreeMap<String, Digest> = BTreeMap::new();
        let mut dim_tree_parts: Vec<String> = Vec::new();

        for dim_ref in instance.all_refs() {
            let dim = self
                .resolve
                .resolve(org, &dim_ref.dim_type, &dim_ref.name)
                .await?;
            inventory_digests.insert(dim_ref.key(), dim.content_hash);
            variables.insert(
                format!("dim_{}", dim_ref.dim_type),
                Value::Object(dim.sections.into_iter().collect()),
            );
            dim_tree_parts.push(dim_ref.key());
        }
        dim_tree_parts.sort();
        variables.insert(
            "org_name".to_string(),
            Value::String(instance.org().to_string()),
        );
        variables.insert(
            "unit_name".to_string(),
            Value::String(instance.unit().to_string()),
        );
        variables.insert(
            "dim_tree".to_string(),
            Value::String(dim_tree_parts.join("/")),
        );

        let package = compute_package(&self.source, instance.unit().as_str()).await?;

        let inventory_revision = self.source.revision().await?;

        let mut resolution =
            ResolutionManifest::empty(package.content_hash, inventory_revision, config_digest);
        resolution.inventory_digests = inventory_digests;

        for input in inputs {
            match self.resolve_input(input).await? {
                Some((value, revision)) => {
                    variables.insert(input.alias.clone(), value);
                    resolution
                        .consumed_inputs
                        .insert(input.alias.clone(), revision);
                }
                None => {
                    // Not required and the producer has never published -
                    // absence itself is part of the pin set too (an
                    // optional input showing up between `plan` and `apply`
                    // is drift, same as one disappearing), so record it as
                    // "no revision" via simply not inserting - `plan`'s and
                    // `apply`'s `consumed_inputs` maps only agree if both
                    // runs saw the same absence.
                }
            }
        }

        Ok((resolution, variables))
    }

    /// Fetch and validate one `[inputs.<alias>]` entry: missing +
    /// `required` is a hard [`AppError::NotFound`]; missing + optional is
    /// `Ok(None)` (skipped, not defaulted to anything); a `schema_version`
    /// that doesn't satisfy `expects` is a hard [`AppError::Validation`]
    /// regardless of `required` - a producer publishing an incompatible
    /// version is never treated as "absent". Every [`OutputValue::Secret`]
    /// is resolved through `IdentityProvider` before it's exposed to a
    /// runner as a variable - `cubtera-store` never holds a resolved
    /// secret value, only the ref.
    async fn resolve_input(&self, input: &InputRequest) -> AppResult<Option<(Value, Revision)>> {
        let Some(set) = self.store.get_output_set(&input.producer).await? else {
            if input.required {
                return Err(AppError::not_found(
                    "unit state",
                    input.producer.canonical(),
                ));
            }
            return Ok(None);
        };

        if let Some(expects) = &input.expects {
            if !set.satisfies(expects) {
                return Err(AppError::validation(format!(
                    "input '{}': producer {} publishes schema {}, which does not satisfy \
                     the required '{}'",
                    input.alias,
                    input.producer.canonical(),
                    set.schema_version,
                    expects
                )));
            }
        }

        let mut resolved = serde_json::Map::new();
        for (key, value) in &set.values {
            let json = match value {
                OutputValue::Plain(v) => v.clone(),
                OutputValue::Secret(secret) => self.identity.resolve_secret(&secret.0).await?,
            };
            resolved.insert(key.clone(), json);
        }
        Ok(Some((Value::Object(resolved), set.revision)))
    }

    fn mint_id(&self, instance: &InstanceId, salt: &str, now: i64) -> String {
        Digest::of_parts([
            instance.canonical().into_bytes(),
            salt.as_bytes().to_vec(),
            now.to_le_bytes().to_vec(),
        ])
        .to_hex()
    }

    /// `cubtera plan -u <unit> -d <dims...>`: resolve everything, run
    /// `command` (typically `["plan"]`) through the capability-checked
    /// runner, and persist the result as a reviewable [`Plan`] artifact.
    pub async fn plan(&self, req: PlanRequest) -> AppResult<Plan> {
        let caps = self.executor.capabilities(&req.runner_type).await?;
        if !caps.supports_plan_artifact {
            return Err(AppError::validation(format!(
                "runner '{}' does not support plan artifacts - use apply directly for this unit",
                req.runner_type
            )));
        }

        let (mut resolution, variables) = self
            .build_resolution(&req.instance, req.config_digest, &req.inputs)
            .await?;
        resolution.runner_version = self
            .executor
            .resolve_runner_version(&req.runner_type, None)
            .await?;

        let outcome = self
            .executor
            .execute(ExecRequest {
                instance: req.instance.clone(),
                runner_type: req.runner_type.clone(),
                command: req.command.clone(),
                auto_approve: false,
                variables,
                requested_version: None,
                collect_outputs: false,
            })
            .await?;

        let now = self.clock.now_unix_ms();
        let artifact_bytes = serde_json::json!({
            "runner_type": req.runner_type,
            "command": req.command,
            "exit_code": outcome.exit_code,
            "success": outcome.success,
        })
        .to_string()
        .into_bytes();
        let artifact_digest = self.store.put_artifact(&artifact_bytes).await?;

        if !outcome.success {
            return Err(AppError::backend(format!(
                "{} {} exited {}",
                req.runner_type,
                req.command.join(" "),
                outcome.exit_code
            )));
        }

        let plan = Plan {
            id: PlanId::new(self.mint_id(&req.instance, "plan", now)),
            instance: req.instance.clone(),
            resolution,
            artifact_digest,
            diff_summary: format!("{} {} exited 0", req.runner_type, req.command.join(" ")),
            created_at: now,
            expires_at: now + req.ttl_seconds * 1000,
        };
        self.store.put_plan(&plan).await?;
        Ok(plan)
    }

    /// `cubtera apply --plan <id>`: refuse if the plan expired or its pins
    /// no longer match a fresh resolution, take a lease on the instance,
    /// run, record a [`Run`], and (if the manifest asked for it and the
    /// runner can) publish an [`OutputSet`].
    pub async fn apply(&self, plan_id: &PlanId, req: ApplyRequest) -> AppResult<Run> {
        let plan = self
            .store
            .get_plan(plan_id)
            .await?
            .ok_or_else(|| AppError::not_found("plan", plan_id.as_str()))?;

        let now = self.clock.now_unix_ms();
        if plan.is_expired(now) {
            return Err(AppError::validation(format!(
                "plan {plan_id} expired at {}; re-plan before applying",
                plan.expires_at
            )));
        }

        let (mut current, variables) = self
            .build_resolution(&plan.instance, req.config_digest, &req.inputs)
            .await?;
        current.runner_version = self
            .executor
            .resolve_runner_version(&req.runner_type, None)
            .await?;
        if !plan.pins_match(&current) {
            return Err(AppError::validation(format!(
                "plan {plan_id} no longer matches the current resolution (inventory, package, config, or runner version drifted) - re-plan"
            )));
        }

        let op = run_op_for(&req.command);
        let run = Run::queued(
            RunId::new(self.mint_id(&plan.instance, "run", now)),
            plan.instance.clone(),
            op.clone(),
            req.actor.clone(),
            now,
        );
        let mut run = Run {
            plan_ref: Some(plan_id.clone()),
            ..run
        };
        self.store.append_run(&run).await?;

        let caps = self.executor.capabilities(&req.runner_type).await?;
        let want_outputs = req.publish_outputs && caps.collects_outputs && op.publishes();

        let lease = self
            .store
            .acquire_lease(&plan.instance, req.actor.as_str(), req.lease_ttl)
            .await?;
        let exec_result = self
            .executor
            .execute(ExecRequest {
                instance: plan.instance.clone(),
                runner_type: req.runner_type.clone(),
                command: req.command.clone(),
                auto_approve: req.auto_approve,
                variables,
                requested_version: None,
                collect_outputs: want_outputs,
            })
            .await;
        // Best-effort release: a run that failed to even acquire/execute
        // must not wedge the instance for every subsequent apply.
        let _ = self.store.release_lease(lease).await;
        let outcome = exec_result?;

        let mut patch = RunPatch {
            status: Some(if outcome.success {
                RunStatus::Succeeded
            } else {
                RunStatus::Failed
            }),
            finished_at: Some(self.clock.now_unix_ms()),
            exit_code: Some(outcome.exit_code),
            ..Default::default()
        };

        if outcome.success && want_outputs {
            if let Some(raw) = &outcome.outputs {
                if let Some(revision) = self
                    .publish_outputs(&plan.instance, raw, &run.id, current.package_digest, &req)
                    .await?
                {
                    patch.produced_outputs_revision = Some(revision);
                }
            }
        }

        // Best-effort: an `Executor` that captures stdio (`cubtera-server`,
        // never the CLI - see `ExecOutcome::log_bytes`'s doc comment)
        // hands back the run's combined output here; persist it
        // content-addressed and point `Run::logs_ref` at the digest so
        // `GET /v1/{org}/runs/{run_id}/log` can serve it later. A storage
        // failure here must not turn an otherwise-successful run into a
        // failed one.
        if let Some(bytes) = &outcome.log_bytes {
            if let Ok(digest) = self.store.put_artifact(bytes).await {
                patch.logs_ref = Some(digest.to_hex());
            }
        }

        self.store.update_run(&run.id, patch.clone()).await?;
        patch.apply_to(&mut run);

        // Best-effort instance bookkeeping - never fails the run itself,
        // matching v2's "publish is best-effort" policy for the analogous
        // step (`RunService`'s `[outputs]` publish).
        let _ = self
            .update_instance_after_run(&plan.instance, current.package_digest, &run)
            .await;

        // Record every input this run actually consumed - best-effort,
        // same rationale: `cubtera state ls --stale` losing track of one
        // consumer is a visibility gap, never a reason to fail an
        // otherwise-successful apply.
        if outcome.success {
            for input in &req.inputs {
                if let Some(revision) = current.consumed_inputs.get(&input.alias) {
                    let _ = self
                        .store
                        .mark_consumed(&plan.instance, &input.producer, *revision)
                        .await;
                }
            }
        }

        Ok(run)
    }

    async fn publish_outputs(
        &self,
        instance: &InstanceId,
        raw: &Value,
        run_id: &RunId,
        source_hash: Digest,
        req: &ApplyRequest,
    ) -> AppResult<Option<Revision>> {
        let Some(obj) = raw.as_object() else {
            return Ok(None);
        };
        let values: BTreeMap<String, OutputValue> = obj
            .iter()
            .map(|(k, v)| (k.clone(), OutputValue::Plain(v.clone())))
            .collect();
        let set = OutputSet {
            schema_version: req.outputs_schema_version.clone(),
            values,
            produced_by: run_id.clone(),
            source_hash,
            revision: Revision::from_raw(0),
        };
        let revision = self.store.put_output_set(instance, &set).await?;
        Ok(Some(revision))
    }

    async fn update_instance_after_run(
        &self,
        id: &InstanceId,
        package_digest: Digest,
        run: &Run,
    ) -> AppResult<()> {
        let existing = self.store.get_instance(id).await?;
        let (mut instance, expected) = match existing {
            Some(inst) => {
                let expected = Some(inst.spec_revision);
                (inst, expected)
            }
            None => (Instance::new(id.clone(), package_digest), None),
        };
        instance.unit_package = package_digest;
        instance.last_run = Some(run.id.clone());
        if let Some(revision) = run.produced_outputs_revision {
            instance.last_outputs_revision = Some(revision);
        }
        self.store.upsert_instance(&instance, expected).await?;
        Ok(())
    }

    /// `cubtera explain run <run_id>`.
    pub async fn explain(&self, run_id: &RunId) -> AppResult<Run> {
        let mut runs = self
            .store
            .list_runs(RunFilter {
                id: Some(run_id.clone()),
                ..Default::default()
            })
            .await?;
        runs.pop()
            .ok_or_else(|| AppError::not_found("run", run_id.as_str()))
    }
}

/// Hash a unit's current on-disk package (manifest + files, no pinned
/// modules yet - see the caveat where this used to live inline in
/// `build_resolution`). Shared with `bindings::BindingUseCase::status`
/// (P5), which needs the exact same "what would `plan` hash *right now*"
/// computation to tell drifted instances apart from up-to-date ones.
pub(crate) async fn compute_package(
    source: &Arc<dyn SourceRepo>,
    unit: &str,
) -> AppResult<UnitPackage> {
    let unit_tree = source.list_files(unit).await?;
    let (manifest_files, other_files): (Vec<_>, Vec<_>) = unit_tree
        .files
        .into_iter()
        .partition(|(path, _)| path == "manifest.toml" || path == "unit.toml");
    let manifest_bytes = manifest_files
        .into_iter()
        .next()
        .map(|(_, content)| content)
        .unwrap_or_default();
    // No pinned-module resolution yet - wiring `[runner] modules` ->
    // `SourceRepo::resolve_module` -> `PinnedModule` is a real gap
    // (tracked, not hidden): every unit in `example/units` is
    // self-contained, so `pinned_modules: vec![]` is honest for what this
    // phase actually exercises, not a silent shortcut on a case that
    // matters today.
    Ok(UnitPackage::compute(&manifest_bytes, &other_files, vec![]))
}

fn run_op_for(command: &[String]) -> RunOp {
    match command.first().map(String::as_str) {
        Some("plan") => RunOp::Plan,
        Some("apply") => RunOp::Apply,
        Some("destroy") => RunOp::Destroy,
        Some("init") => RunOp::Init,
        Some(other) => RunOp::Other(other.to_string()),
        None => RunOp::Other(String::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ExecCapabilities, ExecOutcome, InventoryPort, RawSections};
    use async_trait::async_trait;
    use cubtera_kernel::DimRef;
    use cubtera_source::FsSource;
    use cubtera_store::SqliteStore;
    use std::sync::atomic::{AtomicI64, Ordering};
    use tempfile::TempDir;

    struct FakeInventory;

    #[async_trait]
    impl InventoryPort for FakeInventory {
        async fn get_raw(
            &self,
            _org: &str,
            dim_type: &str,
            name: &str,
        ) -> AppResult<Option<RawSections>> {
            if dim_type == "dome" && name == "prod" {
                let mut sections = RawSections::new();
                sections.insert(
                    "meta".to_string(),
                    serde_json::json!({"region": "us-east-1"}),
                );
                Ok(Some(sections))
            } else {
                Ok(None)
            }
        }

        async fn get_raw_defaults(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Option<RawSections>> {
            Ok(None)
        }

        async fn get_raw_schema(&self, _org: &str, _dim_type: &str) -> AppResult<Option<Value>> {
            Ok(None)
        }

        async fn list_names(&self, _org: &str, _dim_type: &str) -> AppResult<Vec<String>> {
            Ok(vec!["prod".to_string()])
        }

        async fn list_includes(
            &self,
            _org: &str,
            _dim_type: &str,
            _name: &str,
        ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
            Ok(Vec::new())
        }

        async fn list_default_includes(
            &self,
            _org: &str,
            _dim_type: &str,
        ) -> AppResult<Vec<cubtera_model::IncludeEntry>> {
            Ok(Vec::new())
        }
    }

    struct FakeExecutor {
        supports_plan_artifact: bool,
        collects_outputs: bool,
        fail: bool,
        outputs: Option<Value>,
    }

    impl FakeExecutor {
        fn ok() -> Self {
            Self {
                supports_plan_artifact: true,
                collects_outputs: true,
                fail: false,
                outputs: Some(serde_json::json!({"vpc_id": "vpc-1"})),
            }
        }
    }

    #[async_trait]
    impl Executor for FakeExecutor {
        async fn capabilities(&self, _runner_type: &str) -> AppResult<ExecCapabilities> {
            Ok(ExecCapabilities {
                supports_plan_artifact: self.supports_plan_artifact,
                collects_outputs: self.collects_outputs,
                pins_version: false,
                needs_identity: false,
            })
        }

        async fn resolve_runner_version(
            &self,
            runner_type: &str,
            _requested_version: Option<&str>,
        ) -> AppResult<String> {
            Ok(format!("{runner_type}-fake-1.0"))
        }

        async fn execute(&self, req: ExecRequest) -> AppResult<ExecOutcome> {
            Ok(ExecOutcome {
                exit_code: if self.fail { 1 } else { 0 },
                success: !self.fail,
                runner_version: format!("{}-fake-1.0", req.runner_type),
                outputs: if req.collect_outputs {
                    self.outputs.clone()
                } else {
                    None
                },
                log_bytes: None,
            })
        }
    }

    /// Returns a fixed, then ever-increasing, timestamp on each call -
    /// deterministic, but lets a test push `apply`'s clock reading past a
    /// short-lived plan's `expires_at`.
    struct SeqClock {
        next: AtomicI64,
        step: i64,
    }

    impl SeqClock {
        fn new(start: i64, step: i64) -> Self {
            Self {
                next: AtomicI64::new(start),
                step,
            }
        }
    }

    impl Clock for SeqClock {
        fn now_unix_ms(&self) -> i64 {
            self.next.fetch_add(self.step, Ordering::SeqCst)
        }
    }

    fn instance() -> InstanceId {
        InstanceId::try_new(
            Ident::parse("cubtera").unwrap(),
            Ident::parse("network").unwrap(),
            [DimRef::parse("dome:prod").unwrap()],
            [],
        )
        .unwrap()
    }

    async fn unit_source(contents: &str) -> (TempDir, Arc<FsSource>) {
        let tmp = TempDir::new().unwrap();
        let unit_dir = tmp.path().join("network");
        tokio::fs::create_dir_all(&unit_dir).await.unwrap();
        tokio::fs::write(unit_dir.join("manifest.toml"), b"type=\"tofu\"")
            .await
            .unwrap();
        tokio::fs::write(unit_dir.join("main.tf"), contents)
            .await
            .unwrap();
        let source = Arc::new(FsSource::new(tmp.path()));
        (tmp, source)
    }

    fn use_case(
        source: Arc<FsSource>,
        store: Arc<SqliteStore>,
        executor: FakeExecutor,
        clock: SeqClock,
    ) -> RunUseCase {
        RunUseCase::new(
            ResolveUseCase::new(Arc::new(FakeInventory)),
            source,
            store,
            Arc::new(executor),
            Arc::new(clock),
            Arc::new(FakeIdentity),
        )
    }

    /// Errors on any secret ref - no test in this module publishes a
    /// `[outputs] sensitive = [...]` entry, so resolution should never be
    /// reached; a real `EnvIdentityProvider` lives in `cubtera-identity`.
    struct FakeIdentity;

    #[async_trait]
    impl crate::ports::IdentityProvider for FakeIdentity {
        async fn resolve_secret(&self, secret_ref: &str) -> AppResult<Value> {
            Err(AppError::backend(format!(
                "FakeIdentity cannot resolve {secret_ref:?}"
            )))
        }
    }

    #[tokio::test]
    async fn plan_rejects_a_runner_without_plan_capability() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let mut executor = FakeExecutor::ok();
        executor.supports_plan_artifact = false;
        let uc = use_case(source, store, executor, SeqClock::new(1000, 1000));

        let err = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "bash".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn plan_then_apply_round_trip_succeeds_and_publishes_outputs() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let uc = use_case(
            source,
            store.clone(),
            FakeExecutor::ok(),
            SeqClock::new(1000, 1000),
        );

        let plan = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![],
            })
            .await
            .unwrap();
        assert_eq!(plan.resolution.runner_version, "tofu-fake-1.0");
        assert!(!plan.resolution.inventory_digests.is_empty());

        let run = uc
            .apply(
                &plan.id,
                ApplyRequest {
                    runner_type: "tofu".into(),
                    command: vec!["apply".into()],
                    auto_approve: true,
                    actor: Ident::parse("ci").unwrap(),
                    config_digest: Digest::of(b"cfg"),
                    publish_outputs: true,
                    outputs_schema_version: semver::Version::parse("1.0.0").unwrap(),
                    lease_ttl: Duration::from_secs(60),
                    inputs: vec![],
                },
            )
            .await
            .unwrap();

        assert_eq!(run.status, RunStatus::Succeeded);
        assert_eq!(run.exit_code, Some(0));
        assert_eq!(run.produced_outputs_revision, Some(Revision::from_raw(1)));

        let published = store.get_output_set(&instance()).await.unwrap().unwrap();
        assert_eq!(
            published.values.get("vpc_id"),
            Some(&OutputValue::Plain(serde_json::json!("vpc-1")))
        );

        let stored_instance = store.get_instance(&instance()).await.unwrap().unwrap();
        assert_eq!(stored_instance.last_run, Some(run.id.clone()));

        let explained = uc.explain(&run.id).await.unwrap();
        assert_eq!(explained.id, run.id);
        assert_eq!(explained.status, RunStatus::Succeeded);
    }

    #[tokio::test]
    async fn apply_rejects_an_expired_plan() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        // A big step means `apply`'s expiry-check clock read lands well
        // past a plan created with a tiny ttl.
        let uc = use_case(
            source,
            store,
            FakeExecutor::ok(),
            SeqClock::new(1000, 10_000_000),
        );

        let plan = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 1,
                inputs: vec![],
            })
            .await
            .unwrap();

        let err = uc
            .apply(
                &plan.id,
                ApplyRequest {
                    runner_type: "tofu".into(),
                    command: vec!["apply".into()],
                    auto_approve: true,
                    actor: Ident::parse("ci").unwrap(),
                    config_digest: Digest::of(b"cfg"),
                    publish_outputs: false,
                    outputs_schema_version: semver::Version::parse("1.0.0").unwrap(),
                    lease_ttl: Duration::from_secs(60),
                    inputs: vec![],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    #[tokio::test]
    async fn apply_rejects_when_unit_package_drifted_since_plan() {
        let (tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let uc = use_case(source, store, FakeExecutor::ok(), SeqClock::new(1000, 1000));

        let plan = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![],
            })
            .await
            .unwrap();

        // The unit's own files change on disk between `plan` and `apply` -
        // the exact scenario `Plan::pins_match` exists to catch.
        tokio::fs::write(
            tmp.path().join("network").join("main.tf"),
            "variable \"x\" {}\nvariable \"y\" {}\n",
        )
        .await
        .unwrap();

        let err = uc
            .apply(
                &plan.id,
                ApplyRequest {
                    runner_type: "tofu".into(),
                    command: vec!["apply".into()],
                    auto_approve: true,
                    actor: Ident::parse("ci").unwrap(),
                    config_digest: Digest::of(b"cfg"),
                    publish_outputs: false,
                    outputs_schema_version: semver::Version::parse("1.0.0").unwrap(),
                    lease_ttl: Duration::from_secs(60),
                    inputs: vec![],
                },
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }

    fn producer_instance() -> InstanceId {
        InstanceId::try_new(
            Ident::parse("cubtera").unwrap(),
            Ident::parse("platform").unwrap(),
            [DimRef::parse("dome:prod").unwrap()],
            [],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn apply_resolves_a_required_input_and_records_the_consumed_revision() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());

        // Producer publishes directly - this test is about the consumer
        // side (`InputRequest` resolution/`mark_consumed`), not about
        // `publish_outputs` (covered by
        // `plan_then_apply_round_trip_succeeds_and_publishes_outputs`).
        let published = OutputSet {
            schema_version: semver::Version::parse("1.2.0").unwrap(),
            values: BTreeMap::from([(
                "vpc_id".to_string(),
                OutputValue::Plain(serde_json::json!("vpc-1")),
            )]),
            produced_by: RunId::new("run-producer".to_string()),
            source_hash: Digest::of(b"producer"),
            revision: Revision::from_raw(0),
        };
        let first_revision = store
            .put_output_set(&producer_instance(), &published)
            .await
            .unwrap();

        let uc = use_case(
            source,
            store.clone(),
            FakeExecutor::ok(),
            SeqClock::new(1000, 1000),
        );
        let input = InputRequest {
            alias: "platform".to_string(),
            producer: producer_instance(),
            expects: Some(semver::VersionReq::parse("^1.0").unwrap()),
            required: true,
        };

        let plan = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![input.clone()],
            })
            .await
            .unwrap();
        assert_eq!(
            plan.resolution.consumed_inputs.get("platform"),
            Some(&first_revision)
        );

        let run = uc
            .apply(
                &plan.id,
                ApplyRequest {
                    runner_type: "tofu".into(),
                    command: vec!["apply".into()],
                    auto_approve: true,
                    actor: Ident::parse("ci").unwrap(),
                    config_digest: Digest::of(b"cfg"),
                    publish_outputs: false,
                    outputs_schema_version: semver::Version::parse("1.0.0").unwrap(),
                    lease_ttl: Duration::from_secs(60),
                    inputs: vec![input],
                },
            )
            .await
            .unwrap();
        assert_eq!(run.status, RunStatus::Succeeded);

        // Not stale yet - the consumer just applied against the producer's
        // only published revision.
        let org = Ident::parse("cubtera").unwrap();
        assert!(store.list_stale_consumers(&org).await.unwrap().is_empty());

        // Producer moves on to a new revision without the consumer
        // re-applying - `state ls --stale`'s exact scenario.
        store
            .put_output_set(&producer_instance(), &published)
            .await
            .unwrap();
        let stale = store.list_stale_consumers(&org).await.unwrap();
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].consumer, instance());
        assert_eq!(stale[0].producer, producer_instance());
        assert_eq!(stale[0].consumed_revision, first_revision);
    }

    #[tokio::test]
    async fn plan_fails_when_a_required_input_was_never_published() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let uc = use_case(source, store, FakeExecutor::ok(), SeqClock::new(1000, 1000));

        let err = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![InputRequest {
                    alias: "platform".to_string(),
                    producer: producer_instance(),
                    expects: None,
                    required: true,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::NotFound { .. }));
    }

    #[tokio::test]
    async fn plan_skips_a_missing_optional_input_without_erroring() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let uc = use_case(source, store, FakeExecutor::ok(), SeqClock::new(1000, 1000));

        let plan = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![InputRequest {
                    alias: "platform".to_string(),
                    producer: producer_instance(),
                    expects: None,
                    required: false,
                }],
            })
            .await
            .unwrap();
        assert!(!plan.resolution.consumed_inputs.contains_key("platform"));
    }

    #[tokio::test]
    async fn plan_fails_when_the_producers_schema_version_does_not_satisfy_expects() {
        let (_tmp, source) = unit_source("variable \"x\" {}").await;
        let store = Arc::new(SqliteStore::open_in_memory().unwrap());
        let published = OutputSet {
            schema_version: semver::Version::parse("2.0.0").unwrap(),
            values: BTreeMap::new(),
            produced_by: RunId::new("run-producer".to_string()),
            source_hash: Digest::of(b"producer"),
            revision: Revision::from_raw(0),
        };
        store
            .put_output_set(&producer_instance(), &published)
            .await
            .unwrap();

        let uc = use_case(source, store, FakeExecutor::ok(), SeqClock::new(1000, 1000));
        let err = uc
            .plan(PlanRequest {
                instance: instance(),
                runner_type: "tofu".into(),
                command: vec!["plan".into()],
                actor: Ident::parse("ci").unwrap(),
                config_digest: Digest::of(b"cfg"),
                ttl_seconds: 3600,
                inputs: vec![InputRequest {
                    alias: "platform".to_string(),
                    producer: producer_instance(),
                    expects: Some(semver::VersionReq::parse("^1.0").unwrap()),
                    required: true,
                }],
            })
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));
    }
}
