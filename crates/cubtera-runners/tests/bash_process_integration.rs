//! Regression test for the bash runner + `TokioProcessRunner` combination.
//!
//! `TokioProcessRunner::exec` sets the child's cwd to `ProcessSpec::working_dir`.
//! `BashRunner::build_args` must therefore address the script relative to that
//! same directory - not via whatever path was used to *locate* it - or the
//! path gets resolved twice when `working_dir` is itself relative (as it is
//! whenever `tempFolderPath` in `config.toml` is a relative path, which the
//! shipped `example/config.toml` deliberately does).

use cubtera_core::ports::{ProcessRunner, RunContext, RunnerStrategy};
use cubtera_domain::{Manifest, RunParams, Unit};
use cubtera_runners::{BashRunner, TokioProcessRunner};
use std::path::PathBuf;

/// Directory relative to this crate's manifest dir (which is also the test
/// binary's cwd), so a relative `PathBuf` naturally reproduces the bug: if
/// the script path were resolved against the process's cwd a second time
/// (after `current_dir` already moved there), the script would no longer be
/// found.
fn relative_work_dir() -> PathBuf {
    PathBuf::from("target/tmp/bash_process_integration")
}

#[tokio::test]
async fn bash_runner_executes_script_when_working_dir_is_relative() {
    let work_dir = relative_work_dir();
    std::fs::create_dir_all(&work_dir).unwrap();
    std::fs::write(
        work_dir.join("run.sh"),
        "#!/bin/sh\necho ran from bash runner\n",
    )
    .unwrap();

    let strategy = BashRunner::new();
    let unit = Unit::new("script", "cubtera", Manifest::new(vec![], "bash"));
    let ctx = RunContext::new(work_dir.clone());
    let params = RunParams::new(&work_dir).with_command("apply");

    let args = strategy.build_args(&unit, &ctx, &params).await.unwrap();
    let program = strategy.binary(&unit, &ctx, &params).await.unwrap();

    let spec = cubtera_core::ports::ProcessSpec {
        program,
        args,
        working_dir: work_dir.clone(),
        env: Default::default(),
    };

    let process = TokioProcessRunner::new();
    let output = process.exec(&spec).await.unwrap();

    std::fs::remove_dir_all(&work_dir).ok();

    assert!(
        output.success(),
        "expected the script to run successfully, got exit code {}",
        output.exit_code
    );
}
