//! End-to-end tests for `cubtera fleet status`/`cubtera drift` (v3, P5)
//! against the `example/` fixture - same convention as
//! `e2e_validate_fleet.rs`, for the `Binding`/`Selector` command surface.

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
fn fleet_status_reports_desired_for_every_match_never_applied() {
    cli()
        .args([
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "env.name == 'prod'",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("dc:prod-use1"))
        .stdout(predicate::str::contains("desired (never applied)"));
}

#[test]
fn fleet_status_json_reports_every_matching_instance() {
    let output = cli()
        .args([
            "--json",
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "env.name == 'prod'",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let items: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let items = items.as_array().unwrap();
    assert!(!items.is_empty());
    for item in items {
        assert_eq!(item["state"], "Desired");
        assert!(item["instance"].as_str().unwrap().contains("env:prod"));
    }
}

#[test]
fn fleet_status_selector_matching_nothing_reports_empty() {
    cli()
        .args([
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "dc.name == 'does-not-exist'",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No matching instances"));
}

#[test]
fn fleet_status_exclude_filters_a_specific_dc_out() {
    let output = cli()
        .args([
            "--json",
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "dome.name == 'prod'",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let all: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let all_count = all.as_array().unwrap().len();
    assert!(all_count >= 2, "expected multiple dcs under dome:prod");

    let output = cli()
        .args([
            "--json",
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "dome.name == 'prod'",
            "--exclude",
            "dc:prod-use1",
        ])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let filtered: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    let filtered = filtered.as_array().unwrap();
    assert_eq!(filtered.len(), all_count - 1);
    assert!(filtered
        .iter()
        .all(|i| !i["instance"].as_str().unwrap().contains("dc:prod-use1")));
}

/// `cubtera drift` only reports `PackageDrifted`/`Orphaned` - a purely
/// `Desired` fleet (nothing ever applied) is drift-free by definition, and
/// the command must exit `0`.
#[test]
fn drift_is_clean_and_exits_zero_when_nothing_has_ever_been_applied() {
    cli()
        .args([
            "drift",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "env.name == 'prod'",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No matching instances"));
}

#[test]
fn fleet_status_rejects_a_malformed_selector() {
    cli()
        .args([
            "fleet",
            "status",
            "-u",
            "tflike_v3_fixture",
            "-s",
            "dc.status ==",
        ])
        .assert()
        .failure();
}
