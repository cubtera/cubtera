//! End-to-end tests for `cubtera plan`/`cubtera apply`/`cubtera explain
//! run` (v3, P4-run) against the real `tofu` binary and a provider-free
//! fixture unit (`example/units/tflike_v3_fixture`) - same
//! outer-layer-of-the-test-pyramid role as `e2e_validate_fleet.rs`, but
//! exercising the whole `ExecutorBridge` -> `cubtera-exec` ->
//! `TfLikeRunner` -> real process chain instead of `cubtera-app`'s unit
//! tests, which only ever run against a `FakeExecutor`.
//!
//! Skips (rather than fails) if `tofu` isn't on `PATH` - the same
//! graceful-skip convention `cubtera-exec`'s own
//! `opentofu_apply_consumes_an_injected_dimension_variable` test uses, for
//! the same reason: this is exercising real `tofu` behavior, not
//! something `cubtera` itself should ever fake out.
//!
//! `plan`/`apply` deliberately run without `--json`: `tofu`'s own process
//! inherits this CLI's stdout (docs/specs/2026-09-03-cubtera-v3-architecture.md
//! §7's "inherited stdio" requirement, ported unchanged from v2 - it's
//! what keeps colored runner output and interactive prompts working), so
//! it's interleaved with whatever `cubtera` itself prints and `--json`'s
//! single-JSON-value-on-stdout contract can't be honored while a runner
//! is actually invoked. `cubtera explain run` never invokes a runner, so
//! its `--json` output is clean and used here for the final check.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Same isolation convention as `e2e.rs`/`e2e_validate_fleet.rs::cli()`,
/// except the temp/store paths are supplied by the caller so every
/// `cubtera` invocation in one test shares the same materialized
/// workspace and SQLite store - required for `apply --plan <id>` to find
/// the `Plan` `plan` just persisted.
fn cli(temp_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    cmd.current_dir(repo_root())
        .env("CUBTERA_TEMP_PATH", temp_path)
        .env("CUBTERA_STORE_PATH", temp_path.join("store.sqlite"))
        .args(["-c", "example/config.toml"]);
    cmd
}

fn tofu_available() -> bool {
    std::process::Command::new("tofu")
        .arg("version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Pulls the value off the first line starting with `label` (e.g.
/// `"Plan:"`) out of one of this CLI's human-readable reports - robust
/// against the runner's own inherited-stdio output being interleaved with
/// it, unlike trying to parse the whole stream as one JSON value.
fn extract_field<'a>(stdout: &'a str, label: &str) -> &'a str {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .map(str::trim)
        .unwrap_or_else(|| panic!("no line starting with {label:?} in:\n{stdout}"))
}

#[test]
fn plan_apply_explain_round_trip_against_the_tflike_fixture() {
    if !tofu_available() {
        eprintln!("skipping: tofu not installed");
        return;
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    let plan_assert = cli(&temp_path)
        .args(["plan", "-u", "tflike_v3_fixture", "-d", "dome:prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dim_dome_echo"));
    let stdout = String::from_utf8(plan_assert.get_output().stdout.clone()).unwrap();
    let plan_id = extract_field(&stdout, "Plan:").to_string();

    let apply_assert = cli(&temp_path)
        .args([
            "apply",
            "-u",
            "tflike_v3_fixture",
            "-d",
            "dome:prod",
            "--plan",
            &plan_id,
            "--auto-approve",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(apply_assert.get_output().stdout.clone()).unwrap();
    assert_eq!(extract_field(&stdout, "Status:"), "Succeeded");
    assert_eq!(extract_field(&stdout, "Exit code:"), "0");
    let run_id = extract_field(&stdout, "Run:").to_string();
    // `[outputs] publish = true` + `tofu` (`collects_outputs`) should have
    // published an `OutputSet` after this successful apply.
    assert!(stdout.contains("Outputs rev:"));

    cli(&temp_path)
        .args(["--json", "explain", "run", &run_id])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""status": "Succeeded""#))
        .stdout(predicate::str::contains("dome:prod"));
}

#[test]
fn apply_rejects_a_plan_id_that_does_not_exist() {
    if !tofu_available() {
        eprintln!("skipping: tofu not installed");
        return;
    }

    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.path().to_path_buf();

    cli(&temp_path)
        .args([
            "apply",
            "-u",
            "tflike_v3_fixture",
            "-d",
            "dome:prod",
            "--plan",
            "does-not-exist",
            "--auto-approve",
        ])
        .assert()
        .failure()
        .code(4); // EXIT_NOT_FOUND
}
