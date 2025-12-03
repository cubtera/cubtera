//! Runner-related domain types
//!
//! Types for representing runner execution parameters and results.

/// Parameters for runner execution
#[derive(Debug, Clone)]
pub struct RunParams {
    /// Working directory
    pub work_dir: String,
    /// Command to execute (e.g., "plan", "apply")
    pub command: Vec<String>,
    /// Environment variables
    pub env_vars: Vec<(String, String)>,
    /// Auto-approve flag
    pub auto_approve: bool,
}

impl Default for RunParams {
    fn default() -> Self {
        Self {
            work_dir: ".".to_string(),
            command: Vec::new(),
            env_vars: Vec::new(),
            auto_approve: false,
        }
    }
}

impl RunParams {
    /// Create new run parameters with work directory
    pub fn new(work_dir: impl Into<String>) -> Self {
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
            .with_auto_approve(true);

        assert_eq!(params.work_dir, "/tmp/work");
        assert_eq!(params.command, vec!["plan"]);
        assert!(params.auto_approve);
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

