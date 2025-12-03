//! Terraform runner

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::Runner;
use cubtera_domain::{RunParams, RunResult, Unit};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant};
use tracing::{debug, info};

use crate::tfswitch;

/// Terraform runner
pub struct TerraformRunner {
    version: Option<String>,
    /// Lock port for init command (prevents parallel inits)
    lock_port: u16,
}

impl TerraformRunner {
    /// Create a new Terraform runner
    pub fn new(version: Option<String>) -> Self {
        Self {
            version,
            lock_port: 65432,
        }
    }

    /// Create with custom lock port
    pub fn with_lock_port(mut self, port: u16) -> Self {
        self.lock_port = port;
        self
    }

    /// Get the terraform binary path, downloading if necessary
    async fn get_binary_path(&self, params: &RunParams) -> AppResult<PathBuf> {
        // Check if custom runner_command is specified in params
        if let Some(custom_path) = &params.runner_command {
            info!("Using custom terraform binary: {}", custom_path);
            return Ok(PathBuf::from(custom_path));
        }

        // Use version from params, or from runner config, or "latest"
        let version = params
            .version
            .as_ref()
            .or(self.version.as_ref())
            .map(|s| s.to_string())
            .unwrap_or_else(|| "latest".to_string());

        info!("Using Terraform version: {}", version);
        
        // Run blocking tfswitch in a separate thread to avoid runtime conflicts
        tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
            .await
            .map_err(|e| AppError::runner(format!("Task join error: {}", e)))?
    }

    /// Acquire lock for init command
    fn acquire_init_lock(&self) -> Option<TcpListener> {
        let delay = rand::random::<u64>() % 400 + 800;
        
        loop {
            match TcpListener::bind(("127.0.0.1", self.lock_port)) {
                Ok(listener) => return Some(listener),
                Err(_) => {
                    info!("Waiting for init lock (port {})...", self.lock_port);
                    std::thread::sleep(Duration::from_millis(delay));
                }
            }
        }
    }
}

#[async_trait]
impl Runner for TerraformRunner {
    fn name(&self) -> &str {
        "terraform"
    }

    async fn init(&self) -> AppResult<()> {
        // Pre-download terraform if version is known
        if let Some(version) = &self.version {
            info!("Pre-downloading Terraform {}...", version);
            let version = version.clone();
            tokio::task::spawn_blocking(move || tfswitch::tf_switch(&version))
                .await
                .map_err(|e| AppError::runner(format!("Task join error: {}", e)))??;
        }
        Ok(())
    }

    async fn execute(&self, _unit: &Unit, params: &RunParams) -> AppResult<RunResult> {
        let start = Instant::now();

        // Get terraform binary
        let tf_path = self.get_binary_path(params).await?;

        // Check if this is an init command - needs locking
        let is_init = params.command.first().map(|s| s.as_str()) == Some("init");
        let _lock = if is_init {
            self.acquire_init_lock()
        } else {
            None
        };

        let mut cmd = Command::new(&tf_path);
        cmd.current_dir(&params.work_dir);

        // Add command arguments
        for arg in &params.command {
            cmd.arg(arg);
        }

        // Add auto-approve for apply/destroy
        if params.auto_approve {
            let has_apply_or_destroy = params
                .command
                .iter()
                .any(|c| c == "apply" || c == "destroy");
            if has_apply_or_destroy {
                cmd.arg("-auto-approve");
            }
        }

        // Add extra args if specified
        if let Some(extra_args) = &params.extra_args {
            for arg in extra_args.split_whitespace() {
                cmd.arg(arg);
            }
        }

        // Add TF-specific environment variables
        cmd.env("TF_IN_AUTOMATION", "true");
        cmd.env("TF_INPUT", "0");

        // Add user-specified environment variables
        for (key, value) in &params.env_vars {
            cmd.env(key, value);
        }

        debug!(
            "Executing: {} {} (in {})",
            tf_path.display(),
            params.command.join(" "),
            params.work_dir.display()
        );

        // Execute with inherited stdio for interactive output
        let status = cmd
            .status()
            .map_err(|e| AppError::runner(format!("Failed to execute terraform: {}", e)))?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Lock is automatically released here when _lock goes out of scope

        Ok(RunResult {
            exit_code: status.code().unwrap_or(-1),
            stdout: String::new(), // Using inherited stdio
            stderr: String::new(),
            duration_ms,
        })
    }
}
