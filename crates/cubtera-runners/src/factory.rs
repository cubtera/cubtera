//! Runner factory

use crate::{BashRunner, HelmRunner, OpenTofuRunner, TerraformRunner};
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::{RunnerFactory, RunnerStrategy};

/// Default runner factory
pub struct DefaultRunnerFactory {
    /// Default Terraform version
    pub tf_version: Option<String>,
    /// Default OpenTofu version
    pub tofu_version: Option<String>,
}

impl DefaultRunnerFactory {
    /// Create a new runner factory
    pub fn new() -> Self {
        Self {
            tf_version: None,
            tofu_version: None,
        }
    }

    /// Set default Terraform version
    pub fn with_tf_version(mut self, version: String) -> Self {
        self.tf_version = Some(version);
        self
    }

    /// Set default OpenTofu version
    pub fn with_tofu_version(mut self, version: String) -> Self {
        self.tofu_version = Some(version);
        self
    }
}

impl Default for DefaultRunnerFactory {
    fn default() -> Self {
        Self::new()
    }
}

impl RunnerFactory for DefaultRunnerFactory {
    fn create_strategy(&self, runner_type: &str) -> AppResult<Box<dyn RunnerStrategy>> {
        match runner_type.to_lowercase().as_str() {
            "tf" | "terraform" => Ok(Box::new(TerraformRunner::new(self.tf_version.clone()))),
            "tofu" | "opentofu" => Ok(Box::new(OpenTofuRunner::new(self.tofu_version.clone()))),
            "bash" | "sh" => Ok(Box::new(BashRunner::new())),
            "helm" => Ok(Box::new(HelmRunner::new())),
            other => Err(AppError::runner(format!("Unknown runner type: {}", other))),
        }
    }

    fn available_runners(&self) -> Vec<&str> {
        vec!["terraform", "opentofu", "bash", "helm"]
    }
}
