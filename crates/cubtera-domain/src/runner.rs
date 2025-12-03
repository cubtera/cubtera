//! Runner-related domain types
//!
//! Types for representing runner execution parameters and results.

use std::path::PathBuf;

/// Parameters for runner execution
#[derive(Debug, Clone)]
pub struct RunParams {
    /// Working directory
    pub work_dir: PathBuf,
    /// Command to execute (e.g., ["plan"], ["apply"])
    pub command: Vec<String>,
    /// Environment variables
    pub env_vars: Vec<(String, String)>,
    /// Auto-approve flag
    pub auto_approve: bool,
    /// Runner version (e.g., "1.6.6", "latest")
    pub version: Option<String>,
    /// Custom runner binary path (if set, version is ignored)
    pub runner_command: Option<String>,
    /// Extra arguments to pass to runner
    pub extra_args: Option<String>,
    /// State backend type (e.g., "s3", "local")
    pub state_backend: Option<String>,
    /// Lock port for parallel execution control
    pub lock_port: u16,
}

impl Default for RunParams {
    fn default() -> Self {
        Self {
            work_dir: PathBuf::from("."),
            command: Vec::new(),
            env_vars: Vec::new(),
            auto_approve: false,
            version: None,
            runner_command: None,
            extra_args: None,
            state_backend: None,
            lock_port: 65432,
        }
    }
}

impl RunParams {
    /// Create new run parameters with work directory
    pub fn new(work_dir: impl Into<PathBuf>) -> Self {
        Self {
            work_dir: work_dir.into(),
            ..Default::default()
        }
    }

    /// Add a command argument
    pub fn with_command(mut self, cmd: impl Into<String>) -> Self {
        self.command.push(cmd.into());
        self
    }

    /// Add multiple command arguments
    pub fn with_commands(mut self, cmds: Vec<String>) -> Self {
        self.command.extend(cmds);
        self
    }

    /// Add environment variable
    pub fn with_env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env_vars.push((key.into(), value.into()));
        self
    }

    /// Set auto-approve
    pub fn with_auto_approve(mut self, auto_approve: bool) -> Self {
        self.auto_approve = auto_approve;
        self
    }

    /// Set version
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Set custom runner command/binary path
    pub fn with_runner_command(mut self, cmd: impl Into<String>) -> Self {
        self.runner_command = Some(cmd.into());
        self
    }

    /// Set extra arguments
    pub fn with_extra_args(mut self, args: impl Into<String>) -> Self {
        self.extra_args = Some(args.into());
        self
    }

    /// Set state backend
    pub fn with_state_backend(mut self, backend: impl Into<String>) -> Self {
        self.state_backend = Some(backend.into());
        self
    }

    /// Set lock port
    pub fn with_lock_port(mut self, port: u16) -> Self {
        self.lock_port = port;
        self
    }
}

/// Result of a runner execution
#[derive(Debug, Clone)]
pub struct RunResult {
    /// Exit code (0 = success)
    pub exit_code: i32,
    /// Standard output
    pub stdout: String,
    /// Standard error
    pub stderr: String,
    /// Duration in milliseconds
    pub duration_ms: u64,
}

impl RunResult {
    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.exit_code == 0
    }

    /// Create a successful result
    pub fn success(stdout: String, duration_ms: u64) -> Self {
        Self {
            exit_code: 0,
            stdout,
            stderr: String::new(),
            duration_ms,
        }
    }

    /// Create a failed result
    pub fn failure(exit_code: i32, stderr: String, duration_ms: u64) -> Self {
        Self {
            exit_code,
            stdout: String::new(),
            stderr,
            duration_ms,
        }
    }
}

/// State backend type
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StateBackend {
    /// Local file backend
    Local,
    /// AWS S3 backend
    S3,
    /// Google Cloud Storage backend
    Gcs,
    /// Azure Blob Storage backend
    AzureBlob,
    /// HTTP backend
    Http,
    /// Consul backend
    Consul,
    /// Custom/unknown backend
    Custom(String),
}

impl StateBackend {
    /// Parse from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "local" => Self::Local,
            "s3" => Self::S3,
            "gcs" => Self::Gcs,
            "azurerm" | "azure" | "azureblob" => Self::AzureBlob,
            "http" | "https" => Self::Http,
            "consul" => Self::Consul,
            other => Self::Custom(other.to_string()),
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::Local => "local",
            Self::S3 => "s3",
            Self::Gcs => "gcs",
            Self::AzureBlob => "azurerm",
            Self::Http => "http",
            Self::Consul => "consul",
            Self::Custom(s) => s,
        }
    }
}

impl Default for StateBackend {
    fn default() -> Self {
        Self::Local
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_params_builder() {
        let params = RunParams::new("/tmp/work")
            .with_command("plan")
            .with_env("TF_VAR_env", "prod")
            .with_auto_approve(true)
            .with_version("1.6.6");

        assert_eq!(params.work_dir, PathBuf::from("/tmp/work"));
        assert_eq!(params.command, vec!["plan"]);
        assert!(params.auto_approve);
        assert_eq!(params.version, Some("1.6.6".to_string()));
    }

    #[test]
    fn test_run_result_success() {
        let result = RunResult::success("output".to_string(), 100);
        assert!(result.is_success());
        assert_eq!(result.exit_code, 0);
    }

    #[test]
    fn test_run_result_failure() {
        let result = RunResult::failure(1, "error".to_string(), 50);
        assert!(!result.is_success());
        assert_eq!(result.exit_code, 1);
    }

    #[test]
    fn test_state_backend_from_str() {
        assert_eq!(StateBackend::from_str("s3"), StateBackend::S3);
        assert_eq!(StateBackend::from_str("local"), StateBackend::Local);
        assert!(matches!(
            StateBackend::from_str("custom"),
            StateBackend::Custom(_)
        ));
    }
}

