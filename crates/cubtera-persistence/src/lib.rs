//! Cubtera Persistence Layer
//!
//! Repository implementations for different storage backends.
//!
//! MongoDB support was removed entirely in P2 (see
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9): the deployment log
//! and unit state ports are now always backed by `cubtera-store`'s SQLite
//! `Store` (`crate::sqlite`), and inventory has no backend choice left -
//! FS is the only `InventoryRepository` adapter.

#[cfg(feature = "fs")]
pub mod fs;

pub mod sqlite;

mod factory;

pub use factory::*;
