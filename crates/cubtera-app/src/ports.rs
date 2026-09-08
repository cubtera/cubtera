//! Ports `cubtera-app`'s use cases depend on. Every I/O boundary this
//! crate needs lives here as a trait; adapters live in leaf crates (a
//! thin bridge onto the existing `cubtera-persistence` FS adapter for
//! P3, a native SQLite/FS adapter of its own once v2's `cubtera-core` is
//! retired - see docs/specs/2026-09-03-cubtera-v3-architecture.md section 9).

use crate::error::AppResult;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;

/// Raw, adapter-supplied sections for one dimension record - deliberately
/// "dumb" like v2's `cubtera_domain::RawDimension`: no gap-fill, no parent
/// resolution, no schema checking. All of that is `ResolveUseCase`'s job,
/// operating on `cubtera_model::Dimension::assemble`.
pub type RawSections = BTreeMap<String, Value>;

/// Read-only inventory access `cubtera-app`'s use cases need. A subset of
/// v2's `cubtera_core::ports::InventoryRepository` (no `save_raw`/
/// `delete_raw` - resolve/validate never write) using this crate's own
/// types so `cubtera-app` never has to depend on `cubtera-core`.
#[async_trait]
pub trait InventoryPort: Send + Sync {
    /// Fetch the raw record for a dimension by type and name.
    async fn get_raw(
        &self,
        org: &str,
        dim_type: &str,
        name: &str,
    ) -> AppResult<Option<RawSections>>;

    /// Fetch the raw defaults record for a dimension type (".default").
    async fn get_raw_defaults(&self, org: &str, dim_type: &str) -> AppResult<Option<RawSections>>;

    /// Fetch the JSON-schema for a dimension type (its ".schema" record's
    /// "meta" section), if one is defined.
    async fn get_raw_schema(&self, org: &str, dim_type: &str) -> AppResult<Option<Value>>;

    /// List all dimension names of a given type (excludes reserved names).
    async fn list_names(&self, org: &str, dim_type: &str) -> AppResult<Vec<String>>;
}

/// Wall-clock access, injected rather than called directly
/// (`std::time::SystemTime::now()`), so `RunUseCase`'s pin/expiry checks
/// are deterministic under test - the same rationale as
/// `cubtera-config`'s injectable `ConfigSource`.
pub trait Clock: Send + Sync {
    /// Milliseconds since the Unix epoch.
    fn now_unix_ms(&self) -> i64;
}

/// A real wall-clock `Clock`, for production wiring.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> i64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

/// What a runner declares it can do - mirrors `cubtera_exec::RunnerCapabilities`
/// field-for-field, duplicated rather than imported so `cubtera-app` never
/// has to depend on the concrete `cubtera-exec` adapter (only on this
/// port); the CLI's `Executor` bridge is what actually owns a
/// `cubtera-exec` `RunnerStrategy` and translates between the two shapes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExecCapabilities {
    pub supports_plan_artifact: bool,
    pub collects_outputs: bool,
    pub pins_version: bool,
    pub needs_identity: bool,
}

/// What `RunUseCase` asks an `Executor` to do: run `command` against
/// `instance`'s workspace with `variables` exposed however the runner
/// exposes dimension-derived data (`TF_VAR_*`, `CUBTERA_IN_*`, ...).
#[derive(Debug, Clone)]
pub struct ExecRequest {
    pub instance: cubtera_kernel::InstanceId,
    pub runner_type: String,
    pub command: Vec<String>,
    pub auto_approve: bool,
    pub variables: BTreeMap<String, Value>,
    pub requested_version: Option<String>,
    /// Ask the runner to also gather+normalize this run's outputs (only
    /// meaningful when `capabilities().collects_outputs`).
    pub collect_outputs: bool,
}

/// What actually happened, reported back to `RunUseCase` for `Run`
/// persistence.
#[derive(Debug, Clone, Default)]
pub struct ExecOutcome {
    pub exit_code: i32,
    pub success: bool,
    /// The resolved runner binary/version string, for
    /// `ResolutionManifest::runner_version` (e.g. `"tofu"` or
    /// `"/root/.cubtera/tf/1.9.0/terraform"`).
    pub runner_version: String,
    /// Normalized flat `{name: value}` outputs, present only when
    /// `ExecRequest::collect_outputs` was set and the runner actually
    /// collected them.
    pub outputs: Option<Value>,
}

/// Port: actually run a unit's command. Implemented in `crates/cubtera` by
/// bridging to `cubtera-exec`'s `RunnerStrategy`/`Workspace`/`ProcessRunner`
/// - `cubtera-app` never depends on `cubtera-exec` directly, matching the
/// `InventoryPortBridge` pattern already used for P3's inventory access.
#[async_trait]
pub trait Executor: Send + Sync {
    /// Declared capabilities for a runner type (`"tf"`/`"tofu"`/`"bash"`),
    /// checked *before* running anything - the same "reject at validation
    /// time, not after apply" contract `cubtera validate` already applies
    /// to `[outputs] publish = true`.
    async fn capabilities(&self, runner_type: &str) -> AppResult<ExecCapabilities>;

    /// Resolve the runner binary/version string *without* running anything
    /// - `RunUseCase` needs this both at `plan` time (to pin
    /// `ResolutionManifest::runner_version`) and again right before
    /// `apply` (to check that pin hasn't drifted), independently of
    /// whether `execute` ends up being called at all.
    async fn resolve_runner_version(
        &self,
        runner_type: &str,
        requested_version: Option<&str>,
    ) -> AppResult<String>;

    /// Run `req.command` and report what happened.
    async fn execute(&self, req: ExecRequest) -> AppResult<ExecOutcome>;
}
