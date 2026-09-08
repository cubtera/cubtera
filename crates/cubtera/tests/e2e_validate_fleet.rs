//! End-to-end tests for `cubtera validate`/`cubtera fleet ls` (P3's first
//! `cubtera-app`-backed commands) against the `example/` fixture - same
//! outer-layer-of-the-test-pyramid role as `e2e.rs`, just for the v3
//! command surface instead of v2's `im`/`run`.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

/// Same isolation convention as `e2e.rs::cli()`: an isolated temp folder
/// and SQLite store per invocation so parallel tests never collide.
fn cli() -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    let temp_dir = tempfile::tempdir().unwrap();
    let temp_path = temp_dir.into_path();
    cmd.current_dir(repo_root())
        .env("CUBTERA_TEMP_PATH", &temp_path)
        .env("CUBTERA_STORE_PATH", temp_path.join("store.sqlite"))
        .args(["-c", "example/config.toml"]);
    cmd
}

#[test]
fn fleet_ls_lists_every_configured_dim_type() {
    cli()
        .args(["fleet", "ls"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dome:prod"))
        .stdout(predicate::str::contains("env:prod"))
        .stdout(predicate::str::contains("dc:prod-use1"));
}

#[test]
fn fleet_ls_shows_resolved_parent_chain() {
    cli()
        .args(["fleet", "ls", "--dim-type", "dc"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "dome:prod -> env:prod -> dc:prod-use1",
        ));
}

#[test]
fn fleet_ls_json_includes_content_hash() {
    let output = cli()
        .args(["--json", "fleet", "ls", "--dim-type", "dome"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let items: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let items = items.as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert!(item["content_hash"].as_str().unwrap().len() == 64);
        assert!(item["key"].as_str().unwrap().starts_with("dome:"));
    }
}

#[test]
fn validate_reports_every_dimension_valid_for_the_clean_fixture() {
    cli()
        .args(["validate"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dimension(s) checked"));
}

#[test]
fn validate_json_reports_valid_true() {
    let output = cli().args(["--json", "validate"]).assert().success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["valid"], true);
    assert!(value["results"].as_array().unwrap().len() > 5);
}

#[test]
fn validate_scoped_to_one_dim_type_only_checks_that_type() {
    let output = cli()
        .args(["--json", "validate", "--dim-type", "dome"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let results = value["results"].as_array().unwrap();
    assert!(!results.is_empty());
    for r in results {
        assert!(r["key"].as_str().unwrap().starts_with("dome:"));
    }
}
