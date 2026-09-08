//! Native v3 FS adapters for `cubtera_app::ports::{InventoryPort, UnitPort}`.
//!
//! Both are from-scratch ports of v2's `cubtera_persistence::fs::{
//! FsInventoryRepository, FsUnitRepository}` - not wrappers around them -
//! so `cubtera`/`cubtera-server` can eventually stop depending on
//! `cubtera-core`/`cubtera-domain`/`cubtera-persistence` at all (P7's
//! `p7-delete-old` item). Until that rewiring lands, this crate is unused
//! by any binary; it exists so the native adapters can be built and tested
//! independently of the larger CLI/server rewiring.

mod dimension;
mod unit;

pub use dimension::FsInventoryPort;
pub use unit::FsUnitPort;
