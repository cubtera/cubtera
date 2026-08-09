//! Runs the shared `UnitStateRepository` contract suite (see
//! `tests/support_unit_state/mod.rs`) against `FsUnitStateRepository`.

#[path = "support_unit_state/mod.rs"]
mod support_unit_state;

use cubtera_core::ports::UnitStateRepository;
use cubtera_persistence::fs::FsUnitStateRepository;
use std::sync::Arc;

/// Each test gets its own empty temp directory (leaked so it outlives the
/// test - same tradeoff `tests/deployment_log_contract_fs.rs` makes) so
/// runs never see another test's `outputs.json` files.
fn fresh_repo() -> Arc<dyn UnitStateRepository> {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsUnitStateRepository::new(dir.path().to_path_buf());
    std::mem::forget(dir);
    Arc::new(repo)
}

#[tokio::test]
async fn put_then_get_round_trips() {
    support_unit_state::put_then_get_round_trips(fresh_repo()).await;
}

#[tokio::test]
async fn get_missing_key_returns_none() {
    support_unit_state::get_missing_key_returns_none(fresh_repo()).await;
}

#[tokio::test]
async fn put_overwrites_existing_record_for_same_key() {
    support_unit_state::put_overwrites_existing_record_for_same_key(fresh_repo()).await;
}

#[tokio::test]
async fn delete_removes_record() {
    support_unit_state::delete_removes_record(fresh_repo()).await;
}

#[tokio::test]
async fn list_returns_every_record_for_unit() {
    support_unit_state::list_returns_every_record_for_unit(fresh_repo()).await;
}

#[tokio::test]
async fn records_are_scoped_to_their_own_org() {
    support_unit_state::records_are_scoped_to_their_own_org(fresh_repo()).await;
}

#[tokio::test]
async fn get_is_order_independent_for_dims() {
    support_unit_state::get_is_order_independent_for_dims(fresh_repo()).await;
}
