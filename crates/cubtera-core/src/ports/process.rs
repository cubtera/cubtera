//! Process port (interface)
//!
//! The only place a [`RunnerStrategy`](crate::ports::RunnerStrategy) is
//! allowed to spawn an OS process. Adapters run with inherited stdio (not
//! captured) so interactive prompts and colored output (e.g. terraform's)
//! pass through untouched - this is a deliberate parity requirement with v1,
//! not an oversight.

use crate::error::AppResult;
use async_trait::async_trait;
use std::collections::HashMap;
use std::path::PathBuf;

/// A fully-resolved command to execute - no further decisions left to make,
/// only spawning.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    /// Executable to run (resolved path or a name looked up on `PATH`)
    pub program: PathBuf,
    /// Arguments, in order
    pub args: Vec<String>,
    /// Working directory for the child process
    pub working_dir: PathBuf,
    /// Environment variables to set (merged over the parent's environment)
    pub env: HashMap<String, String>,
}

impl ProcessSpec {
    /// Build a `sh -c <command>` spec for inlet/outlet hook commands
    pub fn shell(command: &str, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: PathBuf::from("sh"),
            args: vec!["-c".to_string(), command.to_string()],
            working_dir: working_dir.into(),
            env: HashMap::new(),
        }
    }
}

/// Result of spawning and waiting for a [`ProcessSpec`]
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessOutput {
    /// Process exit code (`-1` if terminated by a signal)
    pub exit_code: i32,
}

impl ProcessOutput {
    /// Whether the process exited with code 0
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Spawns OS processes for runner strategies and inlet/outlet hooks.
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    /// Spawn `spec`, wait for it to complete, and return its exit code.
    async fn exec(&self, spec: &ProcessSpec) -> AppResult<ProcessOutput>;
}
