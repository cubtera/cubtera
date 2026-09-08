//! `cubtera state get/ls/rm` against a SQLite-backed `UnitStateRepository`
//! seeded directly (bypassing an actual `run` + `[outputs] publish = true`,
//! which would need real terraform/tofu credentials) - these commands are
//! pure reads/writes against whatever a producer already published, so
//! seeding the store directly is a faithful test of the CLI plumbing.

use assert_cmd::Command;
use cubtera_store::{LegacyUnitStateRow, SqliteStore};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn cli(store_path: &Path) -> Command {
    let mut cmd = Command::cargo_bin("cubtera").unwrap();
    cmd.current_dir(repo_root())
        .env("CUBTERA_STORE_PATH", store_path)
        .args(["-c", "example/config.toml"]);
    cmd
}

fn fresh_store_path() -> PathBuf {
    tempfile::tempdir()
        .unwrap()
        .into_path()
        .join("store.sqlite")
}

async fn seed(store_path: &Path, unit: &str, dims: &[&str], outputs: serde_json::Value) {
    let store = SqliteStore::open(store_path).unwrap();
    let dims: Vec<String> = dims.iter().map(|s| s.to_string()).collect();
    let ext: Vec<String> = vec![];
    let state_key = LegacyUnitStateRow::state_key("cubtera", unit, &dims, &ext);
    let row = LegacyUnitStateRow {
        org: "cubtera".to_string(),
        unit: unit.to_string(),
        dims,
        ext,
        outputs,
        updated_at: 1_700_000_000,
    };
    store.put_legacy_unit_state(state_key, row).await.unwrap();
}

#[test]
fn state_get_prints_published_outputs_for_exact_key() {
    let store_path = fresh_store_path();
    tokio::runtime::Runtime::new().unwrap().block_on(seed(
        &store_path,
        "network",
        &["dome:prod"],
        serde_json::json!({"vpc_id": "vpc-123"}),
    ));

    cli(&store_path)
        .args(["state", "get", "-u", "network", "-d", "dome:prod"])
        .assert()
        .success()
        .stdout(predicates::str::contains("vpc-123"));
}

#[test]
fn state_get_missing_key_exits_not_found() {
    let store_path = fresh_store_path();

    cli(&store_path)
        .args(["state", "get", "-u", "network", "-d", "dome:prod"])
        .assert()
        .code(4);
}

#[test]
fn state_get_json_emits_full_record() {
    let store_path = fresh_store_path();
    tokio::runtime::Runtime::new().unwrap().block_on(seed(
        &store_path,
        "network",
        &["dome:prod"],
        serde_json::json!({"vpc_id": "vpc-123"}),
    ));

    let output = cli(&store_path)
        .args(["--json", "state", "get", "-u", "network", "-d", "dome:prod"])
        .assert()
        .success();
    let stdout = String::from_utf8(output.get_output().stdout.clone()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed["outputs"]["vpc_id"], "vpc-123");
    assert_eq!(parsed["dims"][0], "dome:prod");
}

#[test]
fn state_ls_lists_every_published_dims_combination() {
    let store_path = fresh_store_path();
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(seed(
        &store_path,
        "network",
        &["dome:prod"],
        serde_json::json!({"a": 1}),
    ));
    rt.block_on(seed(
        &store_path,
        "network",
        &["dome:staging"],
        serde_json::json!({"a": 2}),
    ));

    cli(&store_path)
        .args(["state", "ls", "-u", "network"])
        .assert()
        .success()
        .stdout(predicates::str::contains("dome:prod"))
        .stdout(predicates::str::contains("dome:staging"));
}

#[test]
fn state_ls_on_unpublished_unit_says_so() {
    let store_path = fresh_store_path();

    cli(&store_path)
        .args(["state", "ls", "-u", "network"])
        .assert()
        .success()
        .stdout(predicates::str::contains(
            "No published state found for unit 'network'",
        ));
}

#[test]
fn state_rm_deletes_the_record() {
    let store_path = fresh_store_path();
    tokio::runtime::Runtime::new().unwrap().block_on(seed(
        &store_path,
        "network",
        &["dome:prod"],
        serde_json::json!({"vpc_id": "vpc-123"}),
    ));

    cli(&store_path)
        .args(["state", "rm", "-u", "network", "-d", "dome:prod"])
        .assert()
        .success();

    cli(&store_path)
        .args(["state", "get", "-u", "network", "-d", "dome:prod"])
        .assert()
        .code(4);
}
