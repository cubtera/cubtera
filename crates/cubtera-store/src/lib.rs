//! Cubtera v3 store: the [`Store`] port and its default SQLite adapter.
//!
//! One port replaces three independently-inconsistent v2 mechanisms (a
//! JSONL deployment log, a JSON-per-key unit-state file, and no
//! `Plan`/`Run` persistence at all): instance specs, plan artifacts, run
//! history, published outputs, mutual-exclusion leases, and content-
//! addressed blobs, all behind one transactional, revisioned interface.
//! See docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9.
//!
//! `SqliteStore` is the only adapter in-tree today - it wraps a single
//! `rusqlite::Connection` behind a mutex and runs every operation through
//! `tokio::task::spawn_blocking`, per the workspace's async discipline rule
//! (adapters must not block the tokio reactor).

mod error;
mod legacy;
mod port;
mod sqlite;

pub use error::{StoreError, StoreResult};
pub use legacy::{LegacyDeploymentLogRow, LegacyUnitStateRow};
pub use port::Store;
pub use sqlite::SqliteStore;
