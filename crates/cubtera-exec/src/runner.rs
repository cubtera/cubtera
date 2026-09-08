//! Capability-aware `RunnerStrategy` contract.
//!
//! v2's `cubtera_core::ports::RunnerStrategy` mixed two concerns: the
//! *pipeline* (materialize/transform/inlet/exec/outlet/log, all owned by
//! `RunService`) and the *capability contract* (does this runner even
//! support plan artifacts / output collection / version pinning). Only the
//! second half is this trait's job - `cubtera-app`'s use cases (P4-run) own
//! the pipeline, calling through this trait for the runner-specific parts.
//!
//! This split is also what lets `cubtera validate` reject `[outputs]
//! publish = true` against a runner whose `collects_outputs` is `false` at
//! validation time, instead of silently no-op-ing after `apply` - the exact
//! trap v2's `OpenTofuRunner` fell into by never overriding
//! `collect_outputs`.

use crate::error::ExecResult;
use crate::process::ProcessRunner;
use async_trait::async_trait;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// What a runner type can and can't do - declared once per strategy, not
/// discovered by trial and error after a run fails or silently no-ops.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RunnerCapabilities {
    /// Can this runner's execution be split into a separate `plan`
    /// artifact that `apply --plan <id>` later replays against a pin
    /// check? (P4-run's `Plan`/`Run` objects.)
    pub supports_plan_artifact: bool,
    /// Does the strategy know how to gather this run's outputs itself
    /// (`collect_outputs`), or does the unit have to write
    /// `cubtera_outputs.json` on its own (e.g. via an `outlet_command`)?
    pub collects_outputs: bool,
    /// Does the strategy pin/manage its own binary version (like
    /// Terraform's `tfswitch`), or does it just resolve whatever's on
    /// `PATH`?
    pub pins_version: bool,
    /// Does this runner need resolved credentials injected into its
    /// process env (`cubtera-identity`, P6)?
    pub needs_identity: bool,
}

/// Everything a `RunnerStrategy` needs to build a command: the workspace
/// it's executing in, the raw command/subcommand, and the dimension-derived
/// data the unit should see - exposed differently per runner (tf-like:
/// `TF_VAR_<key>`; bash: `CUBTERA_IN_<KEY>`).
#[derive(Debug, Clone, Default)]
pub struct RunnerContext {
    pub workspace_root: PathBuf,
    pub command: Vec<String>,
    pub auto_approve: bool,
    /// Dimension/extension-derived values the unit's process should see.
    pub variables: BTreeMap<String, Value>,
    /// Passthrough env vars that win over anything a strategy derives from
    /// `variables` (e.g. an explicit `[runner] env` override from the
    /// manifest).
    pub extra_env: BTreeMap<String, String>,
    /// Explicit version pin (from the manifest/CLI), if any.
    pub requested_version: Option<String>,
}

/// The behavior specific to one runner type. Everything pipeline-shaped
/// (materialize files, run inlet/outlet hooks, log the run) is owned by the
/// caller (`cubtera-app`'s use cases); this trait only expresses what
/// differs between terraform/tofu/bash/....
#[async_trait]
pub trait RunnerStrategy: Send + Sync {
    /// Runner name, for logging and `Run.op` metadata.
    fn name(&self) -> &str;

    /// Declared capabilities - checked by `cubtera validate` against the
    /// manifest's `[outputs]`/plan usage before any run happens.
    fn capabilities(&self) -> RunnerCapabilities;

    /// Resolve the executable to run for this context (may download/cache
    /// a pinned version, or just look one up on `PATH`).
    async fn binary(&self, ctx: &RunnerContext) -> ExecResult<PathBuf>;

    /// Build the argument list for `ctx.command`.
    fn build_args(&self, ctx: &RunnerContext) -> ExecResult<Vec<String>>;

    /// Strategy-specific environment variables, derived from
    /// `ctx.variables` plus any strategy defaults. `ctx.extra_env` is
    /// merged in by the caller afterward and always wins.
    fn env_vars(&self, ctx: &RunnerContext) -> BTreeMap<String, String>;

    /// After a successful apply/destroy, write this run's outputs to
    /// `cubtera_outputs.json` under `ctx.workspace_root` (if
    /// `capabilities().collects_outputs`). Default: no-op - the unit is
    /// expected to have written it itself (e.g. via an outlet hook).
    async fn collect_outputs(
        &self,
        _ctx: &RunnerContext,
        _process: &dyn ProcessRunner,
    ) -> ExecResult<()> {
        Ok(())
    }

    /// Normalize a just-collected `cubtera_outputs.json` into the flat
    /// `{name: value}` shape every consumer expects. Default: passthrough
    /// (already-flat JSON, as bash/helm units write themselves).
    fn normalize_outputs(&self, raw: &Value) -> Value {
        raw.clone()
    }
}

/// Merge a strategy's derived env with `ctx.extra_env` (which always wins -
/// it's the more specific, per-invocation override).
pub fn merged_env(
    mut strategy_env: BTreeMap<String, String>,
    ctx: &RunnerContext,
) -> BTreeMap<String, String> {
    for (k, v) in &ctx.extra_env {
        strategy_env.insert(k.clone(), v.clone());
    }
    strategy_env
}
