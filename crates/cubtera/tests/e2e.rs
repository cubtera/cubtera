//! End-to-end CLI tests against the `example/` fixture.
//!
//! These exercise the compiled `cubtera` binary through `assert_cmd`
//! (real process, real args, real exit codes) rather than calling command
//! functions directly - they're the outermost layer of the test pyramid,
//! sitting above the golden inventory tests
//! (`crates/cubtera-persistence/tests/golden_inventory.rs`) and the
//! `InventoryRepository` contract suite
//! (`crates/cubtera-persistence/tests/inventory_contract_fs.rs`).

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// A `cubtera` invocation rooted at the repo root, pointed at
/// `example/config.toml`, with an isolated temp folder per test so parallel
/// tests (and `--dry-run` output) never collide on `example/.cubtera`.
fn cli() -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    // Leak: the dir only needs to outlive this one process invocation, and
    // the OS temp dir gets reaped independently of this test suite.
    let temp_path = temp_dir.into_path();
    // Every command eagerly opens the SQLite store (`Repositories::from_config`),
    // even `im get`/`config`, which never touch it - point it at an isolated
    // per-test path so tests never collide on (or depend on the existence
    // of) the real `~/.cubtera/store.sqlite`.
    let store_path = temp_path.join("store.sqlite");
    cmd.current_dir(repo_root())
        .env("CUBTERA_TEMP_PATH", temp_path)
        .env("CUBTERA_STORE_PATH", store_path)
        .args(["-c", "example/config.toml"]);
    cmd
}

#[test]
fn config_prints_human_readable_summary() {
    cli()
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("Cubtera Configuration"))
        .stdout(predicate::str::contains("cubtera"));
}

#[test]
fn config_json_is_valid_json_with_expected_org() {
    let output = cli().args(["config", "--json"]).assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["org"], "cubtera");
}

#[test]
fn im_get_types_lists_known_dimension_types() {
    cli()
        .args(["im", "get-types"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dc"))
        .stdout(predicate::str::contains("env"))
        .stdout(predicate::str::contains("dome"));
}

#[test]
fn im_get_all_lists_dc_names() {
    cli()
        .args(["im", "get-all", "dc"])
        .assert()
        .success()
        .stdout(predicate::str::contains("prod-use1"));
}

#[test]
fn im_get_returns_gap_filled_json() {
    let output = cli()
        .args(["--json", "im", "get", "dc", "prod-use1"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let dim: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    // prod-use1.json has no "region" of its own - gap-filled from .default:meta.json,
    // pinning the same behavior the golden inventory tests cover at the service layer.
    assert_eq!(dim["meta"]["region"], "us-east-1");
    assert_eq!(dim["parent"], "env:prod");
}

#[test]
fn im_get_missing_dimension_exits_not_found() {
    cli()
        .args(["im", "get", "dc", "does-not-exist"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn im_validate_reports_valid_for_schema_satisfying_dimension() {
    cli()
        .args(["im", "validate", "dc", "prod-use1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Valid"));
}

#[test]
fn im_validate_missing_dimension_exits_not_found() {
    cli()
        .args(["im", "validate", "dc", "does-not-exist"])
        .assert()
        .failure()
        .code(4);
}

#[test]
fn run_dry_run_prints_materialization_plan_without_executing() {
    cli()
        .args([
            "run",
            "-u",
            "bash_unit01",
            "-d",
            "dc:stg1-use2",
            "--dry-run",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Materialization plan"))
        .stdout(predicate::str::contains("cubtera_dim_dc.json"));
}

#[test]
fn run_without_required_dimension_exits_validation_error() {
    cli()
        .args(["run", "-u", "tf_unit01", "--dry-run"])
        .assert()
        .failure()
        .code(5)
        .stderr(predicate::str::contains("missing required dimensions"));
}

#[test]
fn run_denied_by_allow_list_exits_access_denied() {
    // tf_unit01's allowList is ["dome:mgmt", "dome:stg"]; "prod" is neither.
    cli()
        .args(["run", "-u", "tf_unit01", "-d", "dome:prod", "--dry-run"])
        .assert()
        .failure()
        .code(3)
        .stderr(predicate::str::contains("access denied"));
}

#[test]
fn run_allowed_by_allow_list_dry_run_succeeds() {
    cli()
        .args(["run", "-u", "tf_unit01", "-d", "dome:mgmt", "--dry-run"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Materialization plan"));
}
