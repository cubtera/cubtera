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
use std::sync::Arc;

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
/// `cubtera-server` (P7: "authn/authz and log streaming").
///
/// Deliberately *not* `#[async_trait]`-based (plain native `async fn` in
/// the trait, stable since Rust 1.75): nothing needs `Arc<dyn
/// CapturingProcessRunner>` (`ServerExecutor` holds a concrete
/// `TokioCapturingProcessRunner`), and `async_trait`'s macro rewrites this
/// trait's `Arc<dyn Fn(&[u8]) + ...>` parameter's elided lifetime in a way
/// that made an otherwise-sound call site unification fail
/// (`'lifeN` vs `'static`) - a real macro limitation, not a soundness
/// issue in this code.
///
/// Structurally identical to `cubtera_app::ports::LogSink` (both are
/// `Arc<dyn Fn(&[u8]) + Send + Sync>`), duplicated as its own alias
/// rather than imported - `cubtera-exec` never depends on `cubtera-app`
/// (the dependency rule in AGENTS.md) - so `cubtera-server`'s
/// `ServerExecutor` passes `ExecRequest::log_sink` straight through with
/// no conversion needed.
pub type ChunkSink = dyn Fn(&[u8]) + Send + Sync;

#[allow(async_fn_in_trait)]
pub trait CapturingProcessRunner: Send + Sync {
    /// Capture the whole run, no live tailing - equivalent to
    /// `exec_captured_streaming(spec, None)`.
    async fn exec_captured(&self, spec: &ProcessSpec) -> ExecResult<(ProcessOutput, Vec<u8>)> {
        self.exec_captured_streaming(spec, None).await
    }

    /// Same capture, but also invokes `sink` (if given) with each chunk of
    /// combined stdout+stderr as it's produced - the "live" half of P7's
    /// log-streaming requirement. `cubtera-server` forwards these chunks
    /// into a per-`Run` broadcast channel so `GET .../runs/{id}/log/stream`
    /// can tail an in-progress run instead of only ever serving a
    /// finished artifact.
    async fn exec_captured_streaming(
        &self,
        spec: &ProcessSpec,
        sink: Option<Arc<ChunkSink>>,
    ) -> ExecResult<(ProcessOutput, Vec<u8>)>;
}

/// Runs a [`ProcessSpec`] with stdout+stderr piped and captured,
/// interleaved in the order the OS actually delivers them: both streams
/// are read concurrently (`tokio::select!`, fixed-size chunks) rather than
/// stdout-to-completion-then-stderr - required for real live tailing (a
/// chatty stderr while stdout is idle must still show up immediately), and
/// incidentally removes the old sequential-read's theoretical deadlock
/// risk if a process ever filled one pipe's OS buffer while blocked
/// writing to the other.
#[derive(Debug, Clone, Copy, Default)]
pub struct TokioCapturingProcessRunner;

impl TokioCapturingProcessRunner {
    pub fn new() -> Self {
        Self
    }
}

impl CapturingProcessRunner for TokioCapturingProcessRunner {
    async fn exec_captured_streaming(
        &self,
        spec: &ProcessSpec,
        sink: Option<Arc<ChunkSink>>,
    ) -> ExecResult<(ProcessOutput, Vec<u8>)> {
        use std::process::Stdio;
        use tokio::io::AsyncReadExt;
        use tokio::sync::mpsc;

        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        cmd.current_dir(&spec.working_dir);
        cmd.envs(&spec.env);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| ExecError::Process(format!("failed to spawn {}: {e}", spec.program)))?;

        let mut stdout = child
            .stdout
            .take()
            .expect("child spawned with Stdio::piped() stdout");
        let mut stderr = child
            .stderr
            .take()
            .expect("child spawned with Stdio::piped() stderr");

        // Two spawned readers feed one channel, so chunks arrive
        // interleaved in real delivery order without a `select!` loop
        // borrowing a shared buffer across iterations (which, combined
        // with the `dyn Fn` trait object in `sink`, tripped rustc's
        // generator dropck as "borrowed value does not live long enough"
        // - every local here is instead a fully owned `Vec<u8>` handed
        // off by value, sidestepping that entirely).
        let (tx, mut rx) = mpsc::unbounded_channel::<std::io::Result<Vec<u8>>>();

        let out_tx = tx.clone();
        let out_task = tokio::spawn(async move {
            loop {
                let mut buf = vec![0u8; 8192];
                match stdout.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.truncate(n);
                        if out_tx.send(Ok(buf)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = out_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
        let err_tx = tx.clone();
        let err_task = tokio::spawn(async move {
            loop {
                let mut buf = vec![0u8; 8192];
                match stderr.read(&mut buf).await {
                    Ok(0) => break,
                    Ok(n) => {
                        buf.truncate(n);
                        if err_tx.send(Ok(buf)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = err_tx.send(Err(e));
                        break;
                    }
                }
            }
        });
        // Only the readers' cloned senders should keep the channel open -
        // once both tasks finish, `rx.recv()` returns `None`.
        drop(tx);

        let mut combined = Vec::new();
        let mut read_err = None;
        while let Some(item) = rx.recv().await {
            match item {
                Ok(buf) => {
                    combined.extend_from_slice(&buf);
                    if let Some(sink) = &sink {
                        sink(&buf);
                    }
                }
                Err(e) => {
                    read_err.get_or_insert(e);
                }
            };
        }
        let _ = out_task.await;
        let _ = err_task.await;

        let status = child
            .wait()
            .await
            .map_err(|e| ExecError::Process(format!("failed to wait for {}: {e}", spec.program)))?;

        if let Some(e) = read_err {
            return Err(ExecError::Process(format!(
                "failed to read {}'s output: {e}",
                spec.program
            )));
        }

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
    async fn exec_captured_streaming_forwards_chunks_to_the_sink() {
        use std::sync::Mutex;

        let runner = TokioCapturingProcessRunner::new();
        let spec = ProcessSpec::shell("echo out-line; echo err-line >&2", std::env::temp_dir());

        let received: Arc<Mutex<Vec<u8>>> = Arc::new(Mutex::new(Vec::new()));
        let sink_received = received.clone();
        let sink: Arc<ChunkSink> = Arc::new(move |chunk: &[u8]| {
            sink_received.lock().unwrap().extend_from_slice(chunk);
        });

        let (output, combined) = runner
            .exec_captured_streaming(&spec, Some(sink))
            .await
            .unwrap();
        assert!(output.success());

        // Everything the sink saw is a subset of (in this single-chunk-per-line
        // case, exactly) what ended up in the final combined buffer - the
        // live path and the "read to completion" path must never disagree.
        let seen = String::from_utf8(received.lock().unwrap().clone()).unwrap();
        assert!(seen.contains("out-line"));
        assert!(seen.contains("err-line"));
        assert_eq!(seen.into_bytes(), combined);
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
