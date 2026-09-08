//! File system repository implementations
//!
//! `FsDeploymentLogRepository`/`FsUnitStateRepository` were retired in P2:
//! the deployment log and unit state (cross-unit outputs) ports are now
//! always backed by `cubtera-store`'s SQLite `Store` - see
//! `crate::sqlite::{SqliteDeploymentLogRepository, SqliteUnitStateRepository}`
//! and docs/specs/2026-09-03-cubtera-v3-architecture.md ยง9.

mod dimension;
mod unit;
mod workspace;

pub use dimension::FsInventoryRepository;
pub use unit::FsUnitRepository;
pub use workspace::FsWorkspace;
