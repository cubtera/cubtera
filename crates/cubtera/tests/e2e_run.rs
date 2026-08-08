//! Real (non-`--dry-run`) execution tests against the `example/` fixture.
//!
//! Unlike `e2e.rs` (which only exercises `--dry-run`), these tests actually
//! run bash/tf/tofu/helm and check the results they produce - no cloud
//! credentials, no cluster, no interactive login required for any of them.
//! `terraform` self-downloads via `tfswitch` the first time it's used for a
//! given version, so the `tf` test needs network but no pre-installed
//! binary. `tofu` and `helm` have no such auto-install path - those tests
//! skip themselves (printing why) when the binary isn't on `PATH`, mirroring
//! the `CUBTERA_TEST_MONGO_URL` skip pattern used by the Mongo contract
//! tests.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// A `cubtera` invocation rooted at the repo root, pointed at
/// `example/config.toml`, sharing the given temp folder path with every
/// other call built from the same `temp_path` - needed because `tf`/`tofu`
/// require `init` to have already materialized the unit's temp folder
/// before `apply`/`destroy` will run against it.
fn cli(temp_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    cmd.current_dir(repo_root())
        .env("CUBTERA_TEMP_PATH", temp_path)
        .args(["-c", "example/config.toml"]);
    cmd
}

/// A fresh, per-test temp folder root - leaked deliberately (see `e2e.rs`):
/// it only needs to outlive this test's `cubtera` invocations, and the OS
/// temp dir is reaped independently of this test suite.
fn fresh_temp_dir() -> PathBuf {
    tempfile::tempdir().unwrap().into_path()
}

/// Probe whether `name` is runnable on `PATH` (used to skip tofu/helm tests
/// in environments that don't have them installed, e.g. local dev).
fn binary_on_path(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok()
}

/// Parse the `Temp folder: <path>` line `cubtera run` always prints, so
/// tests can locate runner-produced files without hardcoding cubtera's
/// temp-path layout.
fn temp_folder_from_stdout(stdout: &str) -> PathBuf {
    let line = stdout
        .lines()
        .find(|l| l.starts_with("Temp folder: "))
        .expect("cubtera run should print a 'Temp folder: ' line");
    PathBuf::from(line.trim_start_matches("Temp folder: ").trim())
}

#[test]
fn bash_unit_runs_and_prints_resolved_dimension_data() {
    let temp_path = fresh_temp_dir();
    let output = cli(&temp_path)
        .args([
            "run",
            "-u",
            "bash_unit01",
            "-d",
            "dc:stg1-use2",
            "--",
            "deploy",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(stdout.contains("stg1-use2"), "stdout: {stdout}");
    assert!(stdout.contains("us-east-2"), "stdout: {stdout}");
    assert!(
        stdout.contains("Hello from bash_unit01's required include file."),
        "stdout: {stdout}"
    );
    // "service" is declared as optDims but wasn't supplied via -e - the
    // materialized cubtera_dim_service.json is the null placeholder.
    assert!(stdout.contains("not supplied this run"), "stdout: {stdout}");
}

#[test]
fn bash_unit_merges_dc_includes_from_defaults_and_own_dimension() {
    // example/inventory/cubtera/dc ships:
    //  - .default:readme.txt / .default:notice.txt / .default:shared/ (every dc dim)
    //  - stg1-use2:notice.txt (overrides the default notice)
    //  - stg1-use2:extra/ (only this dc dim has it)
    let temp_path = fresh_temp_dir();
    let output = cli(&temp_path)
        .args([
            "run",
            "-u",
            "bash_unit01",
            "-d",
            "dc:stg1-use2",
            "--",
            "deploy",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    // Default-only file: no per-dim override, so the default content survives.
    assert!(
        stdout.contains("Default dc readme - present for every dc dimension"),
        "stdout: {stdout}"
    );
    // Same-named file exists in both .default and stg1-use2 - the
    // dimension's own copy must win on disk.
    assert!(
        stdout.contains("stg1-use2's own notice - it overrides the default"),
        "stdout: {stdout}"
    );
    assert!(
        !stdout.contains("This is the default notice for any dc dimension"),
        "default notice.txt should have been overwritten by stg1-use2's own copy\nstdout: {stdout}"
    );
    // Default-only folder: contents are copied too, not just default files.
    assert!(
        stdout.contains("Shared default info folder"),
        "stdout: {stdout}"
    );
    // Dimension-specific folder (no default equivalent).
    assert!(
        stdout.contains("stg1-use2 dim-specific extra file"),
        "stdout: {stdout}"
    );

    let temp_folder = temp_folder_from_stdout(&stdout);
    assert!(temp_folder.join("readme.txt").exists());
    assert!(temp_folder.join("notice.txt").exists());
    assert!(temp_folder.join("shared/info.txt").exists());
    assert!(temp_folder.join("extra/token.txt").exists());
}

#[test]
fn bash_unit_resolves_optional_dimension_when_supplied() {
    // "service" is an optDims entry - unlike a required dimension, it's
    // resolved the same way as any other `-d`, just not mandatory.
    let temp_path = fresh_temp_dir();
    let output = cli(&temp_path)
        .args([
            "run",
            "-u",
            "bash_unit01",
            "-d",
            "dc:stg1-use2",
            "-d",
            "service:admin",
            "--",
            "deploy",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();

    assert!(
        stdout.contains("resolved via an extra -d"),
        "stdout: {stdout}"
    );
    assert!(stdout.contains("\"dim_service_name\""), "stdout: {stdout}");
    assert!(stdout.contains("\"admin\""), "stdout: {stdout}");
}

#[test]
fn tf_unit_applies_and_creates_local_files() {
    let temp_path = fresh_temp_dir();

    cli(&temp_path)
        .args(["run", "-u", "tf_unit02", "-d", "dc:stg1-use2", "--", "init"])
        .assert()
        .success();

    let output = cli(&temp_path)
        .args([
            "run",
            "-u",
            "tf_unit02",
            "-d",
            "dc:stg1-use2",
            "--auto-approve",
            "--",
            "apply",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let temp_folder = temp_folder_from_stdout(&stdout);

    let example_file = temp_folder.join("example.txt");
    assert!(
        example_file.exists(),
        "expected {example_file:?} to exist after apply"
    );
    assert_eq!(
        std::fs::read_to_string(&example_file).unwrap(),
        "This is a sample file created by Terraform."
    );

    let pet_file = temp_folder.join("pet.txt");
    assert!(pet_file.exists(), "expected {pet_file:?} to exist");
    assert!(std::fs::read_to_string(&pet_file)
        .unwrap()
        .starts_with("Your pet name is: "));

    cli(&temp_path)
        .args([
            "run",
            "-u",
            "tf_unit02",
            "-d",
            "dc:stg1-use2",
            "--auto-approve",
            "--",
            "destroy",
        ])
        .assert()
        .success();
}

#[test]
fn tofu_unit_applies_and_creates_local_file() {
    if !binary_on_path("tofu") {
        eprintln!("skipping tofu_unit_applies_and_creates_local_file: `tofu` not found on PATH");
        return;
    }

    let temp_path = fresh_temp_dir();

    cli(&temp_path)
        .args(["run", "-u", "tf_unit01", "-d", "dome:mgmt", "--", "init"])
        .assert()
        .success();

    let output = cli(&temp_path)
        .args([
            "run",
            "-u",
            "tf_unit01",
            "-d",
            "dome:mgmt",
            "--auto-approve",
            "--",
            "apply",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let temp_folder = temp_folder_from_stdout(&stdout);

    let user_data = temp_folder.join("user_data.json");
    assert!(user_data.exists(), "expected {user_data:?} to exist");
    let content = std::fs::read_to_string(&user_data).unwrap();
    assert!(content.contains("gary"), "content: {content}");
    assert!(content.contains("wendy"), "content: {content}");

    cli(&temp_path)
        .args([
            "run",
            "-u",
            "tf_unit01",
            "-d",
            "dome:mgmt",
            "--auto-approve",
            "--",
            "destroy",
        ])
        .assert()
        .success();
}

#[test]
fn helm_unit_templates_dimension_derived_values() {
    if !binary_on_path("helm") {
        eprintln!(
            "skipping helm_unit_templates_dimension_derived_values: `helm` not found on PATH"
        );
        return;
    }

    let temp_path = fresh_temp_dir();
    cli(&temp_path)
        .args([
            "run",
            "-u",
            "helm_unit01",
            "-d",
            "dc:stg1-use2",
            "--",
            "template",
            ".",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("kind: ConfigMap"))
        .stdout(predicate::str::contains("region: \"us-east-2\""))
        .stdout(predicate::str::contains("environment: \"stg1-use2\""));
}
