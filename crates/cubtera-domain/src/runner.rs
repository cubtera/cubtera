//! Runner-related domain types
//!
//! Types for representing runner execution parameters and results.

use crate::error::{DomainError, DomainResult};
use serde_json::Value;
use std::collections::HashMap;
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
    /// Rendered state backend configuration (JSON for backend HCL generation)
    pub state_backend_config: Option<Value>,
    /// Lock port for parallel execution control
    pub lock_port: u16,
    /// Inlet (pre-runner) command
    pub inlet_command: Option<String>,
    /// Outlet (post-runner) command
    pub outlet_command: Option<String>,
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
            state_backend_config: None,
            lock_port: 65432,
            inlet_command: None,
            outlet_command: None,
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

    /// Set inlet command
    pub fn with_inlet_command(mut self, cmd: impl Into<String>) -> Self {
        self.inlet_command = Some(cmd.into());
        self
    }

    /// Set outlet command
    pub fn with_outlet_command(mut self, cmd: impl Into<String>) -> Self {
        self.outlet_command = Some(cmd.into());
        self
    }

    /// Set state backend config (rendered JSON)
    pub fn with_state_backend_config(mut self, config: Value) -> Self {
        self.state_backend_config = Some(config);
        self
    }
}

/// Render `{{org}}`/`{{unit_name}}`/`{{dim_tree}}`-style handlebars
/// placeholders in every string found anywhere in `template` (recursively
/// through objects/arrays; non-string leaves pass through unchanged). Used
/// for `[state.<backend>]` config sections, e.g.
/// `key = "{{dim_tree}}/{{unit_name}}.tfstate"`. Pure string templating, no
/// I/O - the caller decides where the template and context come from.
pub fn render_state_backend_config(template: &Value, context: &Value) -> DomainResult<Value> {
    let hb = handlebars::Handlebars::new();
    render_value(template, &hb, context)
}

fn render_value(
    value: &Value,
    hb: &handlebars::Handlebars,
    context: &Value,
) -> DomainResult<Value> {
    match value {
        Value::String(s) => hb
            .render_template(s, context)
            .map(Value::String)
            .map_err(|e| DomainError::runner(format!("state backend template render error: {e}"))),
        Value::Array(arr) => arr
            .iter()
            .map(|v| render_value(v, hb, context))
            .collect::<DomainResult<Vec<_>>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(k, v)| render_value(v, hb, context).map(|rv| (k.clone(), rv)))
            .collect::<DomainResult<serde_json::Map<_, _>>>()
            .map(Value::Object),
        other => Ok(other.clone()),
    }
}

/// Result of a runner execution
#[derive(Debug, Clone, Default)]
pub struct RunResult {
    /// Whether execution was successful
    pub success: bool,
    /// Exit code (None if not applicable)
    pub exit_code: Option<i32>,
    /// Standard output (if captured)
    pub output: Option<String>,
    /// Metadata collected during pipeline
    pub metadata: HashMap<String, Value>,
}

impl RunResult {
    /// Check if execution was successful
    pub fn is_success(&self) -> bool {
        self.success
    }

    /// Create a successful result
    pub fn success_result() -> Self {
        Self {
            success: true,
            exit_code: Some(0),
            output: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a successful result with exit code
    pub fn with_exit_code(exit_code: i32) -> Self {
        Self {
            success: exit_code == 0,
            exit_code: Some(exit_code),
            output: None,
            metadata: HashMap::new(),
        }
    }

    /// Create a failed result
    pub fn failure(exit_code: i32) -> Self {
        Self {
            success: false,
            exit_code: Some(exit_code),
            output: None,
            metadata: HashMap::new(),
        }
    }

    /// Add metadata
    pub fn with_metadata(mut self, key: impl Into<String>, value: Value) -> Self {
        self.metadata.insert(key.into(), value);
        self
    }
}

/// State backend type
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StateBackend {
    /// Local file backend
    #[default]
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
    #[allow(clippy::should_implement_trait)] // infallible, not `FromStr`
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_run_params_builder() {
        let params = RunParams::new("/tmp/work")
            .with_command("plan")
            .with_env("TF_VAR_env", "prod")
            .with_auto_approve(true)
            .with_version("1.6.6")
            .with_inlet_command("echo inlet")
            .with_outlet_command("echo outlet");

        assert_eq!(params.work_dir, PathBuf::from("/tmp/work"));
        assert_eq!(params.command, vec!["plan"]);
        assert!(params.auto_approve);
        assert_eq!(params.version, Some("1.6.6".to_string()));
        assert_eq!(params.inlet_command, Some("echo inlet".to_string()));
        assert_eq!(params.outlet_command, Some("echo outlet".to_string()));
    }

    #[test]
    fn test_run_result_success() {
        let result = RunResult::success_result();
        assert!(result.is_success());
        assert_eq!(result.exit_code, Some(0));
    }

    #[test]
    fn test_run_result_failure() {
        let result = RunResult::failure(1);
        assert!(!result.is_success());
        assert_eq!(result.exit_code, Some(1));
    }

    #[test]
    fn test_run_result_with_exit_code() {
        let success = RunResult::with_exit_code(0);
        assert!(success.is_success());

        let failure = RunResult::with_exit_code(1);
        assert!(!failure.is_success());
    }

    #[test]
    fn test_run_result_metadata() {
        let result = RunResult::success_result().with_metadata("key", serde_json::json!("value"));

        assert!(result.metadata.contains_key("key"));
        assert_eq!(
            result.metadata.get("key").unwrap(),
            &serde_json::json!("value")
        );
    }

    #[test]
    fn render_state_backend_config_substitutes_context_recursively() {
        let template = serde_json::json!({
            "bucket": "{{org}}-example-state",
            "key": "{{dim_tree}}/{{unit_name}}.tfstate",
            "region": "us-east-1",
            "tags": ["{{org}}", "static"]
        });
        let context = serde_json::json!({
            "org": "cubtera",
            "unit_name": "network",
            "dim_tree": "env:prod"
        });

        let rendered = render_state_backend_config(&template, &context).unwrap();

        assert_eq!(rendered["bucket"], "cubtera-example-state");
        assert_eq!(rendered["key"], "env:prod/network.tfstate");
        assert_eq!(rendered["region"], "us-east-1");
        assert_eq!(rendered["tags"][0], "cubtera");
        assert_eq!(rendered["tags"][1], "static");
    }

    #[test]
    fn render_state_backend_config_errors_on_unknown_helper() {
        let template = serde_json::json!({"key": "{{#bogus}}x{{/bogus}}"});
        let context = serde_json::json!({});
        assert!(render_state_backend_config(&template, &context).is_err());
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
