//! Adversarial end-to-end tests for the v3 kernel seam
//! (docs/specs/2026-09-03-cubtera-v3-architecture.md ยง4, plan todo
//! `p0-seam`).
//!
//! These exercise the exact attacker-facing surfaces the prior architecture
//! review flagged: `cubtera run -u/-d/-e` (temp-folder path escape) and
//! `cubtera state get/rm` (unit-state path escape), through the real
//! compiled binary, with the same adversarial payloads a fuzzer would try.
//! Every case must fail *before* touching the filesystem outside the
//! test's own isolated `CUBTERA_TEMP_PATH`/`unitStatePath` - it is not
//! enough for the command to merely exit non-zero, it must not have
//! created anything outside the sandboxed temp root either.

use assert_cmd::Command;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Same isolated-temp-dir convention as `e2e.rs::cli()`, but returns the
/// temp dir handle too so tests can assert nothing escaped it.
fn cli_with_temp() -> (Command, PathBuf) {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.into_path();
    cmd.current_dir(repo_root())
        .env("CUBTERA_TEMP_PATH", &temp_path)
        .env("CUBTERA_STORE_PATH", temp_path.join("store.sqlite"))
        .args(["-c", "example/config.toml"]);
    (cmd, temp_path)
}

/// A sibling directory next to `temp_path`, one level up - the payloads
/// below all try to land here (`../evil-marker`) or in `/tmp` directly.
/// If any of them succeed, this file exists after the command runs.
fn escape_marker_path(temp_path: &Path) -> PathBuf {
    temp_path
        .parent()
        .expect("tempdir always has a parent")
        .join("cubtera-e2e-escape-marker")
}

#[test]
fn run_rejects_path_traversal_in_extension() {
    let (mut cmd, temp_path) = cli_with_temp();
    let marker = escape_marker_path(&temp_path);
    let _ = std::fs::remove_file(&marker);

    cmd.args([
        "run",
        "-u",
        "tf_unit01",
        "-d",
        "dome:mgmt",
        "-e",
        "../../../../../../tmp/cubtera-e2e-escape-marker",
        "--dry-run",
        "--",
        "init",
    ])
    .assert()
    .failure();

    assert!(
        !marker.exists(),
        "extension path traversal must not create anything outside the temp root"
    );
}

#[test]
fn run_rejects_path_traversal_in_dimension_value() {
    cli_with_temp()
        .0
        .args([
            "run",
            "-u",
            "tf_unit01",
            "-d",
            "dome:../../../etc",
            "--dry-run",
            "--",
            "init",
        ])
        .assert()
        .failure();
}

#[test]
fn run_rejects_path_traversal_in_unit_name() {
    cli_with_temp()
        .0
        .args([
            "run",
            "-u",
            "../../../etc/passwd",
            "-d",
            "dome:mgmt",
            "--dry-run",
            "--",
            "init",
        ])
        .assert()
        .failure();
}

#[test]
fn run_rejects_malformed_dimension_with_no_colon() {
    cli_with_temp()
        .0
        .args([
            "run",
            "-u",
            "tf_unit01",
            "-d",
            "not-a-dim-ref",
            "--dry-run",
            "--",
            "init",
        ])
        .assert()
        .failure();
}

#[test]
fn run_accepts_well_formed_input_as_a_control_case() {
    // Sanity check that the validation above is actually specific to
    // adversarial input, not accidentally rejecting everything.
    cli_with_temp()
        .0
        .args([
            "run",
            "-u",
            "tf_unit01",
            "-d",
            "dome:mgmt",
            "--dry-run",
            "--",
            "init",
        ])
        .assert()
        .success();
}

#[test]
fn state_get_rejects_path_traversal_in_dimension() {
    cli_with_temp()
        .0
        .args([
            "state",
            "get",
            "-u",
            "network",
            "-d",
            "../../../../../../tmp/x:y",
        ])
        .assert()
        .failure();
}

#[test]
fn state_get_rejects_path_traversal_in_unit_name() {
    cli_with_temp()
        .0
        .args(["state", "get", "-u", "../../../etc/passwd"])
        .assert()
        .failure();
}

#[test]
fn state_rm_rejects_path_traversal_in_extension() {
    cli_with_temp()
        .0
        .args([
            "state",
            "rm",
            "-u",
            "network",
            "-d",
            "dome:mgmt",
            "-e",
            "../../../../../../tmp/x:y",
        ])
        .assert()
        .failure();
}

#[test]
fn state_ls_rejects_path_traversal_in_unit_name() {
    cli_with_temp()
        .0
        .args(["state", "ls", "-u", "../../../etc"])
        .assert()
        .failure();
}
