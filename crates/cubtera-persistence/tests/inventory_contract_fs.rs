//! Runs the shared `InventoryRepository` contract suite (see
//! `tests/support/mod.rs`) against `FsInventoryRepository`. Each test gets
//! its own empty temp directory, so a real backend doesn't need any
//! `example/inventory` fixture data.

mod support;

use cubtera_core::ports::InventoryRepository;
use cubtera_persistence::fs::FsInventoryRepository;
use std::sync::Arc;

fn fresh_repo() -> Arc<dyn InventoryRepository> {
    let dir = tempfile::tempdir().unwrap();
    let repo = FsInventoryRepository::new(dir.path().to_path_buf());
    // Leak the guard: these are short-lived test-only directories under the
    // OS temp dir, and `FsInventoryRepository` only holds a `PathBuf`, so we
    // need the directory to outlive this function without a dangling `TempDir`.
    std::mem::forget(dir);
    Arc::new(repo)
}

#[tokio::test]
async fn save_get_roundtrip() {
    support::save_get_roundtrip(fresh_repo()).await;
}

#[tokio::test]
async fn save_get_roundtrip_multiple_sections() {
    support::save_get_roundtrip_multiple_sections(fresh_repo()).await;
}

#[tokio::test]
async fn get_missing_dimension_returns_none() {
    support::get_missing_dimension_returns_none(fresh_repo()).await;
}

#[tokio::test]
async fn get_defaults_and_schema_absent_by_default() {
    support::get_defaults_and_schema_absent_by_default(fresh_repo()).await;
}

#[tokio::test]
async fn delete_removes_dimension() {
    support::delete_removes_dimension(fresh_repo()).await;
}

#[tokio::test]
async fn delete_missing_dimension_is_noop() {
    support::delete_missing_dimension_is_noop(fresh_repo()).await;
}

#[tokio::test]
async fn list_names_reflects_saved_dimensions() {
    support::list_names_reflects_saved_dimensions(fresh_repo()).await;
}

#[tokio::test]
async fn list_names_empty_for_unknown_type() {
    support::list_names_empty_for_unknown_type(fresh_repo()).await;
}

#[tokio::test]
async fn list_types_and_orgs_reflect_saved_data() {
    support::list_types_and_orgs_reflect_saved_data(fresh_repo()).await;
}
