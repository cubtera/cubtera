//! CLI integration tests for Cubtera
//!
//! These tests run the actual CLI binary and verify its behavior.
//! Tests use the example directory as a fixture for inventory and units.

use assert_cmd::Command;
use predicates::prelude::*;
use std::env;
use std::path::PathBuf;

/// Get the path to the example directory for test fixtures
fn get_example_path() -> PathBuf {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string());
    PathBuf::from(manifest_dir).join("example")
}

/// Get a command configured with the example workspace
fn get_cmd() -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    let example_path = get_example_path();

    // Configure environment for tests
    cmd.env("CUBTERA_WORKSPACE_PATH", example_path.to_str().unwrap())
        .env("CUBTERA_ORG", "cubtera")
        .env("CUBTERA_CONFIG", example_path.join("config.toml").to_str().unwrap())
        .env("CUBTERA_LOG", "error"); // Suppress logging in tests

    cmd
}

// ========== Basic CLI Tests ==========

#[test]
fn test_cli_no_args_shows_help() {
    let mut cmd = get_cmd();

    cmd.assert()
        .failure()
        .stderr(predicate::str::contains("Usage:"));
}

#[test]
fn test_cli_help_flag() {
    let mut cmd = get_cmd();

    cmd.arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Immersive cubic dimensions experience"))
        .stdout(predicate::str::contains("im"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("config"));
}

#[test]
fn test_cli_version_flag() {
    let mut cmd = get_cmd();

    cmd.arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("cubtera"));
}

// ========== Config Command Tests ==========

#[test]
fn test_config_command() {
    let mut cmd = get_cmd();

    cmd.arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace_path"))
        .stdout(predicate::str::contains("org"));
}

#[test]
fn test_config_alias_cfg() {
    let mut cmd = get_cmd();

    cmd.arg("cfg")
        .assert()
        .success()
        .stdout(predicate::str::contains("workspace_path"));
}

// ========== IM (Inventory Management) Command Tests ==========

#[test]
fn test_im_help() {
    let mut cmd = get_cmd();

    cmd.args(["im", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Inventory management"))
        .stdout(predicate::str::contains("getAll"))
        .stdout(predicate::str::contains("getByName"));
}

#[test]
fn test_im_getall_help() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAll", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dim_type"));
}

#[test]
fn test_im_getbyname_help() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getByName", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dim_type"))
        .stdout(predicate::str::contains("dim_name"));
}

#[test]
fn test_im_getall_env() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAll", "env"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsByType"));
}

#[test]
fn test_im_getall_dome() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAll", "dome"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsByType"));
}

#[test]
fn test_im_getall_dc() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAll", "dc"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsByType"));
}

#[test]
fn test_im_getbyname_env_prod() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getByName", "env", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimByName"))
        .stdout(predicate::str::contains("prod"));
}

#[test]
fn test_im_getbyname_dome_prod() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getByName", "dome", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimByName"))
        .stdout(predicate::str::contains("prod"));
}

#[test]
fn test_im_getdefaults_env() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getDefaults", "env"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsDefaultsByType"));
}

#[test]
fn test_im_getorgs() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getOrgs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("orgs"))
        .stdout(predicate::str::contains("cubtera"));
}

#[test]
fn test_im_getbyparent() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getByParent", "dome", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsByParent"));
}

#[test]
fn test_im_getparent() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getParent", "env", "prod"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimParent"));
}

#[test]
fn test_im_getalldata_env() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAllData", "env"])
        .assert()
        .success()
        .stdout(predicate::str::contains("status"))
        .stdout(predicate::str::contains("dimsDataByType"));
}

// ========== Log Command Tests ==========

#[test]
fn test_log_help() {
    let mut cmd = get_cmd();

    cmd.args(["log", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Deployment log"));
}

#[test]
fn test_log_get_help() {
    let mut cmd = get_cmd();

    cmd.args(["log", "get", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("query"));
}

// Note: log get requires dlog_db to be configured, so we just test the help

// ========== Run Command Tests ==========

#[test]
fn test_run_help() {
    let mut cmd = get_cmd();

    cmd.args(["run", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run unit"))
        .stdout(predicate::str::contains("--unit"))
        .stdout(predicate::str::contains("--dim"));
}

#[test]
fn test_run_alias_tf() {
    let mut cmd = get_cmd();

    cmd.args(["tf", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Run unit"));
}

// ========== Environment Variable Tests ==========

#[test]
fn test_custom_org_env() {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    let example_path = get_example_path();

    cmd.env("CUBTERA_WORKSPACE_PATH", example_path.to_str().unwrap())
        .env("CUBTERA_ORG", "customorg")
        .env("CUBTERA_CONFIG", example_path.join("config.toml").to_str().unwrap())
        .env("CUBTERA_LOG", "error")
        .arg("config")
        .assert()
        .success()
        .stdout(predicate::str::contains("customorg"));
}

// ========== Error Handling Tests ==========

#[test]
fn test_invalid_subcommand() {
    let mut cmd = get_cmd();

    cmd.arg("invalid_subcommand")
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn test_im_missing_dim_type() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getAll"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dim_type"));
}

#[test]
fn test_im_getbyname_missing_args() {
    let mut cmd = get_cmd();

    cmd.args(["im", "getByName", "env"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("dim_name"));
}

// ========== Response Format Tests ==========

#[test]
fn test_im_response_is_json() {
    let mut cmd = get_cmd();

    let output = cmd
        .args(["im", "getAll", "env"])
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify it's valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "Output should be valid JSON: {}", stdout);
}

#[test]
fn test_config_response_is_json() {
    let mut cmd = get_cmd();

    let output = cmd
        .arg("config")
        .output()
        .expect("Failed to execute command");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify it's valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&stdout);
    assert!(parsed.is_ok(), "Config output should be valid JSON: {}", stdout);
}

// ========== IM Subcommand Validation Tests ==========

#[test]
fn test_im_validate_help() {
    let mut cmd = get_cmd();

    cmd.args(["im", "validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Validate"));
}

// ========== Dimension Type Tests ==========

#[test]
fn test_im_nonexistent_dim_type() {
    let mut cmd = get_cmd();

    // Nonexistent dim types fail (exit with error)
    cmd.args(["im", "getAll", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Can't read data folder"));
}

#[test]
fn test_im_nonexistent_dim_name() {
    let mut cmd = get_cmd();

    // Nonexistent dim names fail (exit with error)
    cmd.args(["im", "getByName", "env", "nonexistent"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Can't find"));
}

