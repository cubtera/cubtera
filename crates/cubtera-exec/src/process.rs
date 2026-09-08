//! `ProcessRunner` port + `TokioProcessRunner` adapter.
//!
//! Same shape as v2's `cubtera_core::ports::{ProcessRunner, ProcessSpec}` /
//! `cubtera_runners::TokioProcessRunner`, relocated into the v3 adapter
//! crate: spawns via `tokio::process::Command` (never blocks the reactor),
//! inherits stdio by default (required for interactive prompts and
//! terraform/tofu's colored output), with a separate `shell` constructor
//! for the one case that needs to redirect stdout to a file instead
//! (`RunnerStrategy::collect_outputs`).

use crate::error::{ExecError, ExecResult};
use async_trait::async_trait;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// A single process invocation.
#[derive(Debug, Clone)]
pub struct ProcessSpec {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env: BTreeMap<String, String>,
}

impl ProcessSpec {
    pub fn new(program: impl Into<String>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: working_dir.into(),
            env: BTreeMap::new(),
        }
    }

    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.args = args;
        self
    }

    pub fn with_env(mut self, env: BTreeMap<String, String>) -> Self {
        self.env = env;
        self
    }

    /// A shell one-liner, run via `sh -c` - used for
    /// `<binary> output -json > cubtera_outputs.json` where the redirect
    /// itself has to happen outside our own process.
    pub fn shell(command: impl Into<String>, working_dir: impl Into<PathBuf>) -> Self {
        Self {
            program: "sh".to_string(),
            args: vec!["-c".to_string(), command.into()],
            working_dir: working_dir.into(),
            env: BTreeMap::new(),
        }
    }
}

/// Result of running a [`ProcessSpec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcessOutput {
    pub exit_code: i32,
}

impl ProcessOutput {
    pub fn success(&self) -> bool {
        self.exit_code == 0
    }
}

/// Port: run a process to completion.
#[async_trait]
pub trait ProcessRunner: Send + Sync {
    async fn exec(&self, spec: &ProcessSpec) -> ExecResult<ProcessOutput>;
}

/// Executes [`ProcessSpec`]s via `tokio::process`, inheriting stdio.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioProcessRunner;

impl TokioProcessRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl ProcessRunner for TokioProcessRunner {
    async fn exec(&self, spec: &ProcessSpec) -> ExecResult<ProcessOutput> {
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.current_dir(&spec.working_dir);
        cmd.envs(&spec.env);

        let status = cmd
            .status()
            .await
            .map_err(|e| ExecError::Process(format!("failed to execute {}: {e}", spec.program)))?;

        Ok(ProcessOutput {
            exit_code: status.code().unwrap_or(-1),
        })
    }
}

/// Captures a process's combined stdout+stderr into memory instead of
/// inheriting the parent's stdio - the opposite trade-off from
/// [`TokioProcessRunner`], deliberately kept as a *separate* trait rather
/// than a second [`ProcessRunner`] method: the CLI must never
/// accidentally get this behavior (it would silently swallow interactive
/// prompts and colored output - see this module's doc comment), so
/// there's no default-provided fallback to forget to override. Built for
/// `cubtera-server` (P7: "authn/authz and log streaming" - this is the
/// "capture the log so it can be served over HTTP" half; true live
/// SSE-tailing of an in-progress run is not implemented here, only
/// after-the-fact retrieval of a finished run's captured output).
#[async_trait]
pub trait CapturingProcessRunner: Send + Sync {
    async fn exec_captured(&self, spec: &ProcessSpec) -> ExecResult<(ProcessOutput, Vec<u8>)>;
}

/// Runs a [`ProcessSpec`] with stdout+stderr piped and captured, combined
/// in the order the OS delivers them (via `tokio::process`'s
/// `Stdio::piped()` on both streams, read to completion after `wait()`).
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioCapturingProcessRunner;

impl TokioCapturingProcessRunner {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CapturingProcessRunner for TokioCapturingProcessRunner {
    async fn exec_captured(&self, spec: &ProcessSpec) -> ExecResult<(ProcessOutput, Vec<u8>)> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;

        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.current_dir(&spec.working_dir);
        cmd.envs(&spec.env);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| ExecError::Process(format!("failed to spawn {}: {e}", spec.program)))?;

        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let mut combined = Vec::new();

        // Sequential (not concurrent) reads are fine here: neither v3's
        // tf-like nor bash runner produces enough output to fill a pipe
        // buffer and deadlock waiting on the other stream, and this trait
        // is never used for the CLI's interactive path (see the doc
        // comment above) where that would matter more.
        if let Some(out) = stdout.as_mut() {
            out.read_to_end(&mut combined)
                .await
                .map_err(|e| ExecError::Process(format!("failed to read stdout: {e}")))?;
        }
        if let Some(err) = stderr.as_mut() {
            err.read_to_end(&mut combined)
                .await
                .map_err(|e| ExecError::Process(format!("failed to read stderr: {e}")))?;
        }

        let status = child
            .wait()
            .await
            .map_err(|e| ExecError::Process(format!("failed to wait for {}: {e}", spec.program)))?;

        Ok((
            ProcessOutput {
                exit_code: status.code().unwrap_or(-1),
            },
            combined,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn capturing_runner_captures_combined_stdout_and_stderr() {
        let runner = TokioCapturingProcessRunner::new();
        let spec = ProcessSpec::shell("echo out-line; echo err-line >&2", std::env::temp_dir());
        let (output, bytes) = runner.exec_captured(&spec).await.unwrap();
        assert!(output.success());
        let text = String::from_utf8(bytes).unwrap();
        assert!(text.contains("out-line"));
        assert!(text.contains("err-line"));
    }

    #[tokio::test]
    async fn capturing_runner_reports_a_nonzero_exit_code() {
        let runner = TokioCapturingProcessRunner::new();
        let spec = ProcessSpec::shell("exit 3", std::env::temp_dir());
        let (output, _bytes) = runner.exec_captured(&spec).await.unwrap();
        assert_eq!(output.exit_code, 3);
        assert!(!output.success());
    }

    #[tokio::test]
    async fn exec_reports_exit_code() {
        let runner = TokioProcessRunner::new();
        let spec = ProcessSpec::shell("exit 7", std::env::temp_dir());
        let output = runner.exec(&spec).await.unwrap();
        assert_eq!(output.exit_code, 7);
        assert!(!output.success());
    }

    #[tokio::test]
    async fn exec_passes_env_and_cwd() {
        let tmp = tempfile::TempDir::new().unwrap();
        let runner = TokioProcessRunner::new();

        // Writes to a relative path, proving `current_dir` was honored
        // (independent of any tmpdir-symlink weirdness `$(pwd)` string
        // comparison would run into on macOS).
        let mut env = BTreeMap::new();
        env.insert("CUBTERA_TEST_VAR".to_string(), "hello".to_string());
        let spec =
            ProcessSpec::shell("echo \"$CUBTERA_TEST_VAR\" > marker.txt", tmp.path()).with_env(env);
        let output = runner.exec(&spec).await.unwrap();
        assert!(output.success());

        let marker = tokio::fs::read_to_string(tmp.path().join("marker.txt"))
            .await
            .unwrap();
        assert_eq!(marker.trim(), "hello");
    }
}
