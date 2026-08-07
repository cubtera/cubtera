//! Run service
//!
//! Owns the runner pipeline: `init -> prepare/materialize -> transform ->
//! inlet -> execute -> outlet -> log`. Every step a `RunnerStrategy` doesn't
//! override runs identically for every runner type - this is what "разобрать
//! god-trait Runner" in the migration plan means: the pipeline lives in one
//! place instead of being re-implemented (or silently skipped) by each
//! strategy.

use crate::error::AppResult;
use crate::ports::{
    CopyConfig, DeploymentLogEntry, DeploymentLogRepository, PrepareMode, ProcessRunner,
    ProcessSpec, RunContext, RunnerFactory, Workspace,
};
use cubtera_domain::{RunParams, RunResult, Unit};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

/// Orchestrates the full runner pipeline for a unit
pub struct RunService {
    runner_factory: Arc<dyn RunnerFactory>,
    workspace: Arc<dyn Workspace>,
    process: Arc<dyn ProcessRunner>,
    deployment_log: Option<Arc<dyn DeploymentLogRepository>>,
    copy_config: CopyConfig,
}

impl RunService {
    /// Create a new run service
    pub fn new(
        runner_factory: Arc<dyn RunnerFactory>,
        workspace: Arc<dyn Workspace>,
        process: Arc<dyn ProcessRunner>,
        copy_config: CopyConfig,
    ) -> Self {
        Self {
            runner_factory,
            workspace,
            process,
            deployment_log: None,
            copy_config,
        }
    }

    /// Set deployment log repository
    pub fn with_deployment_log(mut self, log: Arc<dyn DeploymentLogRepository>) -> Self {
        self.deployment_log = Some(log);
        self
    }

    /// Run a unit with the specified command, driving the full pipeline:
    /// materialize -> transform -> inlet -> execute -> outlet -> log.
    pub async fn run(
        &self,
        unit: &Unit,
        command: Vec<String>,
        params_override: Option<RunParams>,
    ) -> AppResult<RunResult> {
        let started_at = SystemTime::now();
        let strategy = self
            .runner_factory
            .create_strategy(unit.manifest.runner_type().as_str())?;

        strategy.init().await?;

        let params = params_override
            .unwrap_or_else(|| RunParams::new(&unit.temp_folder).with_commands(command.clone()));

        let mut ctx = RunContext::new(unit.temp_folder.clone());
        tracing::info!(
            runner = strategy.name(),
            unit = %unit.name,
            command = ?params.command,
            "Starting runner pipeline"
        );

        self.prepare(strategy.as_ref(), unit, &params, &mut ctx)
            .await?;
        strategy.transform_files(unit, &ctx).await?;
        self.run_hook(&params.inlet_command, &ctx, "inlet").await?;
        strategy
            .execute(unit, &params, &mut ctx, self.process.as_ref())
            .await?;
        self.run_hook(&params.outlet_command, &ctx, "outlet")
            .await?;

        let result = RunResult {
            success: ctx.exit_code.unwrap_or(0) == 0,
            exit_code: ctx.exit_code,
            output: None,
            metadata: ctx.metadata,
        };

        if let Some(log) = &self.deployment_log {
            if self.should_log_command(&command) {
                let duration_ms = started_at
                    .elapsed()
                    .map(|d| d.as_millis() as u64)
                    .unwrap_or(0);
                let entry = self.create_log_entry(unit, &command, &result, duration_ms);
                if let Err(e) = log.save(&entry).await {
                    tracing::warn!("Failed to save deployment log: {}", e);
                }
            }
        }

        // Cleanup is best-effort: a failure here must not turn a successful
        // apply/destroy into a reported failure.
        if self.copy_config.clean_cache && result.is_success() {
            if let Err(e) = self.workspace.clean(&unit.temp_folder).await {
                tracing::warn!("Failed to clean temp folder after run: {}", e);
            }
        }

        Ok(result)
    }

    /// Run with auto-approve (for apply/destroy)
    pub async fn run_auto_approve(
        &self,
        unit: &Unit,
        command: Vec<String>,
    ) -> AppResult<RunResult> {
        let params = RunParams::new(&unit.temp_folder)
            .with_commands(command)
            .with_auto_approve(true);

        self.run(unit, params.command.clone(), Some(params)).await
    }

    /// Get available runner types
    pub fn available_runners(&self) -> Vec<&str> {
        self.runner_factory.available_runners()
    }

    /// Prepare the temp folder per the strategy's [`PrepareMode`], then apply
    /// the materialization plan (with any strategy-specific steps mixed in)
    /// if this mode calls for it.
    async fn prepare(
        &self,
        strategy: &dyn crate::ports::RunnerStrategy,
        unit: &Unit,
        params: &RunParams,
        ctx: &mut RunContext,
    ) -> AppResult<()> {
        let apply_plan = match strategy.prepare_mode(params, &self.copy_config) {
            PrepareMode::CleanAndMaterialize => {
                self.workspace.clean(&unit.temp_folder).await?;
                true
            }
            PrepareMode::RequireExisting { rematerialize } => {
                if !unit.temp_folder_exists() {
                    return Err(crate::error::AppError::runner(format!(
                        "Temp folder not found: {:?}. Run 'init' first.",
                        unit.temp_folder
                    )));
                }
                rematerialize
            }
        };

        if apply_plan {
            let mut plan = unit.materialize(&self.copy_config.modules_path, None);
            strategy.extend_plan(unit, params, &mut plan);
            self.workspace.apply(&plan).await?;
        }

        ctx.working_dir = unit.temp_folder.clone();
        ctx.set_metadata(
            "prepare",
            serde_json::json!(if apply_plan {
                "materialized"
            } else {
                "verified"
            }),
        );
        Ok(())
    }

    /// Run an inlet/outlet shell hook, if configured, failing the pipeline on non-zero exit.
    async fn run_hook(
        &self,
        command: &Option<String>,
        ctx: &RunContext,
        step_name: &str,
    ) -> AppResult<()> {
        let Some(command) = command else {
            return Ok(());
        };
        let spec = ProcessSpec::shell(command, ctx.working_dir.clone());
        let output = self.process.exec(&spec).await?;
        if !output.success() {
            return Err(crate::error::AppError::runner(format!(
                "{step_name} command failed with exit code: {}",
                output.exit_code
            )));
        }
        Ok(())
    }

    /// Check if command should be logged
    fn should_log_command(&self, command: &[String]) -> bool {
        command
            .first()
            .map(|c| matches!(c.as_str(), "apply" | "destroy"))
            .unwrap_or(false)
    }

    /// Create deployment log entry
    fn create_log_entry(
        &self,
        unit: &Unit,
        command: &[String],
        result: &RunResult,
        duration_ms: u64,
    ) -> DeploymentLogEntry {
        DeploymentLogEntry {
            unit_name: unit.name.clone(),
            org: unit.org.clone(),
            dimensions: unit.dimensions.iter().map(|d| d.key()).collect(),
            command: command.join(" "),
            exit_code: result.exit_code.unwrap_or(-1),
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0),
            duration_ms,
            git_shas: HashMap::new(),
            metadata: result.metadata.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{ProcessOutput, RunnerStrategy};
    use async_trait::async_trait;
    use cubtera_domain::{Manifest, MaterializationPlan};
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeWorkspace {
        applied: Mutex<Vec<MaterializationPlan>>,
        cleaned: Mutex<Vec<PathBuf>>,
    }

    #[async_trait]
    impl Workspace for FakeWorkspace {
        async fn apply(&self, plan: &MaterializationPlan) -> AppResult<()> {
            self.applied.lock().unwrap().push(plan.clone());
            Ok(())
        }
        async fn clean(&self, temp_folder: &Path) -> AppResult<()> {
            self.cleaned.lock().unwrap().push(temp_folder.to_path_buf());
            Ok(())
        }
    }

    #[derive(Default)]
    struct FakeProcess {
        calls: Mutex<Vec<ProcessSpec>>,
        exit_code: i32,
    }

    #[async_trait]
    impl ProcessRunner for FakeProcess {
        async fn exec(&self, spec: &ProcessSpec) -> AppResult<ProcessOutput> {
            self.calls.lock().unwrap().push(spec.clone());
            Ok(ProcessOutput {
                exit_code: self.exit_code,
            })
        }
    }

    /// A minimal strategy that just calls through to the process port,
    /// relying entirely on `RunnerStrategy`'s default `execute`.
    #[derive(Default)]
    struct MinimalStrategy;

    #[async_trait]
    impl RunnerStrategy for MinimalStrategy {
        fn name(&self) -> &str {
            "minimal"
        }

        async fn binary(
            &self,
            _unit: &Unit,
            _ctx: &RunContext,
            _params: &RunParams,
        ) -> AppResult<PathBuf> {
            Ok(PathBuf::from("true"))
        }
    }

    #[derive(Default)]
    struct FakeFactory;

    impl RunnerFactory for FakeFactory {
        fn create_strategy(&self, _runner_type: &str) -> AppResult<Box<dyn RunnerStrategy>> {
            Ok(Box::new(MinimalStrategy))
        }
        fn available_runners(&self) -> Vec<&str> {
            vec!["minimal"]
        }
    }

    fn unit() -> Unit {
        Unit::new("network", "cubtera", Manifest::new(vec![], "tf")).with_temp_folder("/tmp/unit")
    }

    fn copy_config() -> CopyConfig {
        CopyConfig {
            modules_path: PathBuf::from("/modules"),
            plugins_path: PathBuf::from("/plugins"),
            always_copy_files: false,
            clean_cache: false,
        }
    }

    #[tokio::test]
    async fn run_cleans_materializes_and_executes_for_default_prepare_mode() {
        let workspace = Arc::new(FakeWorkspace::default());
        let process = Arc::new(FakeProcess {
            exit_code: 0,
            ..Default::default()
        });
        let factory = Arc::new(FakeFactory);
        let service = RunService::new(factory, workspace.clone(), process.clone(), copy_config());

        let result = service
            .run(&unit(), vec!["plan".to_string()], None)
            .await
            .unwrap();

        assert!(result.is_success());
        assert_eq!(workspace.cleaned.lock().unwrap().len(), 1);
        assert_eq!(workspace.applied.lock().unwrap().len(), 1);
        assert_eq!(process.calls.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn run_propagates_nonzero_exit_code_as_failure() {
        let workspace = Arc::new(FakeWorkspace::default());
        let process = Arc::new(FakeProcess {
            exit_code: 2,
            ..Default::default()
        });
        let factory = Arc::new(FakeFactory);
        let service = RunService::new(factory, workspace, process, copy_config());

        let result = service
            .run(&unit(), vec!["apply".to_string()], None)
            .await
            .unwrap();

        assert!(!result.is_success());
        assert_eq!(result.exit_code, Some(2));
    }

    #[tokio::test]
    async fn run_cleans_temp_folder_after_successful_run_when_clean_cache_enabled() {
        let workspace = Arc::new(FakeWorkspace::default());
        let process = Arc::new(FakeProcess {
            exit_code: 0,
            ..Default::default()
        });
        let factory = Arc::new(FakeFactory);
        let mut config = copy_config();
        config.clean_cache = true;
        let service = RunService::new(factory, workspace.clone(), process, config);

        service
            .run(&unit(), vec!["apply".to_string()], None)
            .await
            .unwrap();

        // Once from `prepare` (CleanAndMaterialize), once from post-run cleanup.
        assert_eq!(workspace.cleaned.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn run_does_not_clean_temp_folder_after_failed_run_even_with_clean_cache_enabled() {
        let workspace = Arc::new(FakeWorkspace::default());
        let process = Arc::new(FakeProcess {
            exit_code: 1,
            ..Default::default()
        });
        let factory = Arc::new(FakeFactory);
        let mut config = copy_config();
        config.clean_cache = true;
        let service = RunService::new(factory, workspace.clone(), process, config);

        service
            .run(&unit(), vec!["apply".to_string()], None)
            .await
            .unwrap();

        // Only the pre-run clean from `prepare`, none from post-run cleanup.
        assert_eq!(workspace.cleaned.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn run_fails_fast_when_inlet_command_fails() {
        let workspace = Arc::new(FakeWorkspace::default());
        let process = Arc::new(FakeProcess {
            exit_code: 1,
            ..Default::default()
        });
        let factory = Arc::new(FakeFactory);
        let service = RunService::new(factory, workspace, process.clone(), copy_config());
        let params = RunParams::new("/tmp/unit")
            .with_command("plan")
            .with_inlet_command("false");

        let result = service
            .run(&unit(), vec!["plan".to_string()], Some(params))
            .await;

        assert!(result.is_err());
        // Only the inlet hook ran; `execute` never got a chance to call the process port.
        assert_eq!(process.calls.lock().unwrap().len(), 1);
    }
}
