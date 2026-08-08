//! OpenTofu runner strategy
//!
//! Similar to Terraform but uses the `tofu` binary, has no version-switch
//! implementation yet, and needs no init lock (tofu's own state locking is
//! sufficient - v1's terraform-only lock port was a workaround, not a
//! general requirement).
//! TODO: Add version management (tofuswitch) similar to terraform

use async_trait::async_trait;
use cubtera_core::error::AppResult;
use cubtera_core::ports::{CopyConfig, PrepareMode, RunContext, RunnerStrategy};
use cubtera_domain::{RunParams, Unit};
use std::path::PathBuf;
use tracing::info;

/// OpenTofu runner strategy
pub struct OpenTofuRunner {
    version: Option<String>,
}

impl OpenTofuRunner {
    /// Create a new OpenTofu strategy
    pub fn new(version: Option<String>) -> Self {
        Self { version }
    }

    fn is_init_command(params: &RunParams) -> bool {
        params.command.first().map(|s| s.as_str()) == Some("init")
    }
}

#[async_trait]
impl RunnerStrategy for OpenTofuRunner {
    fn name(&self) -> &str {
        "opentofu"
    }

    async fn init(&self) -> AppResult<()> {
        if let Some(version) = &self.version {
            info!(
                "Requested OpenTofu version: {} (TODO: implement tofuswitch)",
                version
            );
        }
        Ok(())
    }

    fn prepare_mode(&self, params: &RunParams, copy_config: &CopyConfig) -> PrepareMode {
        if Self::is_init_command(params) {
            PrepareMode::CleanAndMaterialize
        } else {
            PrepareMode::RequireExisting {
                rematerialize: copy_config.always_copy_files,
            }
        }
    }

    async fn binary(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        _params: &RunParams,
    ) -> AppResult<PathBuf> {
        Ok(PathBuf::from("tofu"))
    }

    async fn build_args(
        &self,
        _unit: &Unit,
        _ctx: &RunContext,
        params: &RunParams,
    ) -> AppResult<Vec<String>> {
        let mut args = params.command.clone();

        if params.auto_approve {
            let has_apply_or_destroy = params
                .command
                .iter()
                .any(|c| c == "apply" || c == "destroy");
            if has_apply_or_destroy {
                args.push("-auto-approve".to_string());
            }
        }

        if let Some(extra_args) = &params.extra_args {
            args.extend(extra_args.split_whitespace().map(String::from));
        }

        Ok(args)
    }

    fn env_vars(&self, _unit: &Unit, _params: &RunParams) -> Vec<(String, String)> {
        vec![
            ("TF_IN_AUTOMATION".to_string(), "true".to_string()),
            ("TF_INPUT".to_string(), "0".to_string()),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_mode_cleans_on_init() {
        let strategy = OpenTofuRunner::new(None);
        let params = RunParams::new(".").with_command("init");
        let copy_config = CopyConfig {
            modules_path: PathBuf::new(),
            plugins_path: PathBuf::new(),
            always_copy_files: false,
            clean_cache: false,
        };
        assert_eq!(
            strategy.prepare_mode(&params, &copy_config),
            PrepareMode::CleanAndMaterialize
        );
    }

    #[test]
    fn prepare_mode_requires_existing_on_apply() {
        let strategy = OpenTofuRunner::new(None);
        let params = RunParams::new(".").with_command("apply");
        let copy_config = CopyConfig {
            modules_path: PathBuf::new(),
            plugins_path: PathBuf::new(),
            always_copy_files: false,
            clean_cache: false,
        };
        assert_eq!(
            strategy.prepare_mode(&params, &copy_config),
            PrepareMode::RequireExisting {
                rematerialize: false
            }
        );
    }
}
