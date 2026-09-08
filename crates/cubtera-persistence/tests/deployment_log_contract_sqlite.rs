//! Runs the shared `DeploymentLogRepository` contract suite (see
//! `tests/support_dlog/mod.rs`) against `SqliteDeploymentLogRepository` -
//! the backend that replaced `FsDeploymentLogRepository`/
//! `MongoDeploymentLogRepository` in P2.

#[path = "support_dlog/mod.rs"]
mod support_dlog;

use cubtera_core::ports::DeploymentLogRepository;
use cubtera_persistence::sqlite::SqliteDeploymentLogRepository;
use cubtera_store::SqliteStore;
use std::sync::Arc;

/// Each test gets its own fresh in-memory SQLite database, so runs never
/// see another test's rows (unlike a shared on-disk store, an in-memory
/// `SqliteStore` is naturally isolated per instance).
fn fresh_repo() -> Arc<dyn DeploymentLogRepository> {
    let store = Arc::new(SqliteStore::open_in_memory().unwrap());
    Arc::new(SqliteDeploymentLogRepository::new(store))
}

#[tokio::test]
async fn save_makes_entry_findable() {
    support_dlog::save_makes_entry_findable(fresh_repo()).await;
}

#[tokio::test]
async fn find_on_org_with_no_entries_returns_empty() {
    support_dlog::find_on_org_with_no_entries_returns_empty(fresh_repo()).await;
}

#[tokio::test]
async fn find_filters_by_unit_name() {
    support_dlog::find_filters_by_unit_name(fresh_repo()).await;
}

#[tokio::test]
async fn find_filters_by_dimension() {
    support_dlog::find_filters_by_dimension(fresh_repo()).await;
}

#[tokio::test]
async fn find_orders_newest_first() {
    support_dlog::find_orders_newest_first(fresh_repo()).await;
}

#[tokio::test]
async fn find_respects_limit() {
    support_dlog::find_respects_limit(fresh_repo()).await;
}

#[tokio::test]
async fn find_by_dimensions_requires_every_dimension() {
    support_dlog::find_by_dimensions_requires_every_dimension(fresh_repo()).await;
}

#[tokio::test]
async fn entries_are_scoped_to_their_own_org() {
    support_dlog::entries_are_scoped_to_their_own_org(fresh_repo()).await;
}
