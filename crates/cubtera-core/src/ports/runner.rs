//! Runner strategy port (interface)
//!
//! `RunnerStrategy` only expresses what differs *between* runner types
//! (terraform/opentofu/bash): which binary to run, how to build its
//! arguments and environment, and any file transforms needed before running
//! it. The pipeline itself - materialize, transform, inlet, exec, outlet,
//! log - is owned by `RunService` (see `crate::services::RunService`), not
//! by this trait. This is the split called for by the migration plan's
//! "разобрать god-trait Runner" item: the previous `Runner` trait *was* the
//! pipeline (with I/O-performing default methods each strategy inherited),
//! which meant every override had to re-implement pipeline concerns
//! (removing/copying files) alongside its actual differences.

use crate::error::AppResult;
use crate::ports::{ProcessRunner, ProcessSpec};
use async_trait::async_trait;
use cubtera_domain::{MaterializationPlan, RunParams, Unit};
use serde_json::Value;
use std::collections::HashMap;
use std::path::PathBuf;

/// Context passed through pipeline steps
#[derive(Debug, Clone, Default)]
pub struct RunContext {
    /// Working directory for runner execution
    pub working_dir: PathBuf,
    /// Exit code from runner command
    pub exit_code: Option<i32>,
    /// Metadata collected during pipeline
    pub metadata: HashMap<String, Value>,
}

impl RunContext {
    /// Create new context with working directory
    pub fn new(working_dir: PathBuf) -> Self {
        Self {
            working_dir,
            exit_code: None,
            metadata: HashMap::new(),
        }
    }

    /// Update metadata
    pub fn set_metadata(&mut self, key: impl Into<String>, value: Value) {
        self.metadata.insert(key.into(), value);
    }

    /// Get metadata value
    pub fn get_metadata(&self, key: &str) -> Option<&Value> {
        self.metadata.get(key)
    }
}

/// Configuration for the materialize pipeline step
#[derive(Debug, Clone)]
pub struct CopyConfig {
    /// Path to modules directory
    pub modules_path: PathBuf,
    /// Path to plugins directory
    pub plugins_path: PathBuf,
    /// Always re-materialize files even when the temp folder already exists
    pub always_copy_files: bool,
    /// Clean temp cache after a successful run
    pub clean_cache: bool,
}

/// How `RunService` should prepare a unit's temp folder before materializing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrepareMode {
    /// Remove any existing temp folder, then materialize fresh. This is the
    /// only safe default: a stale temp folder from a previous, differently
    /// configured run must not silently linger.
    CleanAndMaterialize,
    /// Require an existing temp folder (`RunService` fails otherwise);
    /// optionally re-materialize on top of it without cleaning first.
    RequireExisting {
        /// Re-apply the materialization plan over the existing folder
        rematerialize: bool,
    },
}

/// The behavior specific to one runner type (terraform/opentofu/bash/...).
/// Everything else - copying files, running inlet/outlet hooks, logging - is
/// handled once by `RunService` for every strategy.
#[async_trait]
pub trait RunnerStrategy: Send + Sync {
    /// Runner name, for logging
    fn name(&self) -> &str;

    /// One-time setup (e.g. pre-download a pinned binary version)
    async fn init(&self) -> AppResult<()> {
        Ok(())
    }

    /// How to prepare the temp folder for this command. Default: always
    /// clean and materialize fresh, which is correct for one-shot runners
    /// like bash. Terraform/OpenTofu override this to require `init` to
    /// have run first.
    fn prepare_mode(&self, _params: &RunParams, _copy_config: &CopyConfig) -> PrepareMode {
        PrepareMode::CleanAndMaterialize
    }

    /// Add strategy-specific steps to the materialization plan (e.g.
    /// terraform's `cubtera_backend.tf`) before it's applied. Called exactly
    /// when the plan is about to be applied - i.e. not at all when
    /// `prepare_mode` returns `RequireExisting { rematerialize: false }`.
    fn extend_plan(&self, _unit: &Unit, _params: &RunParams, _plan: &mut MaterializationPlan) {}

    /// Transform already-materialized files in the temp folder (e.g.
    /// converting `cubtera_*.json` to `*.auto.tfvars.json`). Runs after the
    /// plan is applied, before `execute`.
    async fn transform_files(&self, _unit: &Unit, _ctx: &RunContext) -> AppResult<()> {
        Ok(())
    }

    /// Resolve the executable to run
    async fn binary(&self, unit: &Unit, ctx: &RunContext, params: &RunParams)
        -> AppResult<PathBuf>;

    /// Build the argument list (default: just the raw command, e.g. `["plan"]`)
    async fn build_args(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        params: &RunParams,
    ) -> AppResult<Vec<String>> {
        Ok(params.command.clone())
    }

    /// Strategy-specific environment variables (default: none)
    fn env_vars(&self, _unit: &Unit, _params: &RunParams) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Resolve and run the command through `process`. The default composes
    /// `binary`/`build_args`/`env_vars` into one [`ProcessSpec`]; override
    /// when the call itself needs extra control (terraform wraps this in an
    /// init lock).
    async fn execute(
        &self,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
        process: &dyn ProcessRunner,
    ) -> AppResult<()> {
        let program = self.binary(unit, ctx, params).await?;
        let args = self.build_args(unit, ctx, params).await?;
        let env = merged_env(self.env_vars(unit, params), params);

        let spec = ProcessSpec {
            program: program.clone(),
            args: args.clone(),
            working_dir: ctx.working_dir.clone(),
            env,
        };
        let output = process.exec(&spec).await?;

        ctx.exit_code = Some(output.exit_code);
        ctx.set_metadata(
            "runner",
            serde_json::json!({
                "binary": program.display().to_string(),
                "command": args,
                "exit_code": output.exit_code,
            }),
        );
        Ok(())
    }
}

/// Merge a strategy's own env vars with the caller-supplied `params.env_vars`
/// (which win on conflicts - they're the more specific, per-invocation override).
pub fn merged_env(
    strategy_env: Vec<(String, String)>,
    params: &RunParams,
) -> HashMap<String, String> {
    let mut env: HashMap<String, String> = strategy_env.into_iter().collect();
    for (k, v) in &params.env_vars {
        env.insert(k.clone(), v.clone());
    }
    env
}

/// Factory for creating runner strategies
pub trait RunnerFactory: Send + Sync {
    /// Create a strategy for the given runner type
    fn create_strategy(&self, runner_type: &str) -> AppResult<Box<dyn RunnerStrategy>>;

    /// Get available runner types
    fn available_runners(&self) -> Vec<&str>;
}
