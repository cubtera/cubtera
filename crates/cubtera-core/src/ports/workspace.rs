//! Workspace port (interface)
//!
//! Executes a [`cubtera_domain::MaterializationPlan`] built by
//! [`cubtera_domain::Unit::materialize`]. The domain only *describes* the
//! filesystem work; this port is where that description turns into actual
//! I/O, keeping the plan itself testable and `--dry-run`-printable without a
//! filesystem.

use crate::error::AppResult;
use async_trait::async_trait;
use cubtera_domain::MaterializationPlan;

/// Applies materialization plans to a real (or fake, in tests) filesystem.
#[async_trait]
pub trait Workspace: Send + Sync {
    /// Execute every step of `plan`, in order. Creates `plan.temp_folder`
    /// first if it doesn't exist. Implementations should be idempotent:
    /// applying the same plan twice must succeed and converge to the same
    /// state.
    async fn apply(&self, plan: &MaterializationPlan) -> AppResult<()>;

    /// Remove a unit's temp working directory, if it exists.
    async fn clean(&self, temp_folder: &std::path::Path) -> AppResult<()>;

    /// Read a file's contents from a unit's (already materialized) working
    /// directory, `None` if it doesn't exist. Used by `RunService` to pick
    /// up a runner's `cubtera_outputs.json` after `execute` without
    /// hardcoding a filesystem read outside this port.
    async fn read_file(&self, path: &std::path::Path) -> AppResult<Option<String>>;
}
