//! Runner port (interface)
//!
//! Trait for infrastructure runners. Implementations live in cubtera-runners.

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::{RunParams, RunResult, Unit};

/// Runner for executing infrastructure code
#[async_trait]
pub trait Runner: Send + Sync {
    /// Get the runner name
    fn name(&self) -> &str;

    /// Initialize the runner (e.g., download binaries, setup environment)
    async fn init(&self) -> AppResult<()>;

    /// Execute a command
    async fn execute(&self, unit: &Unit, params: &RunParams) -> AppResult<RunResult>;

    /// Execute with auto-approve (for apply/destroy)
    async fn execute_auto_approve(&self, unit: &Unit, params: &RunParams) -> AppResult<RunResult> {
        let mut params = params.clone();
        params.auto_approve = true;
        self.execute(unit, &params).await
    }
}

/// Factory for creating runners
pub trait RunnerFactory: Send + Sync {
    /// Create a runner for the given type
    fn create_runner(&self, runner_type: &str) -> AppResult<Box<dyn Runner>>;

    /// Get available runner types
    fn available_runners(&self) -> Vec<&str>;
}

