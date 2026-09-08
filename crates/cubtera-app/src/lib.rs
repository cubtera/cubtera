//! Cubtera v3 application layer: use cases and their ports.
//!
//! Depends only on `cubtera-model`/`cubtera-kernel` (see the crate table in
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง3) - every I/O
//! boundary a use case needs is a trait in [`ports`], implemented by an
//! adapter in a leaf crate (for P3, a thin bridge onto the existing
//! v2 `cubtera-persistence` FS adapter, wired up in `crates/cubtera`).
//!
//! P3 ships two use cases: [`resolve::ResolveUseCase`] (raw record ->
//! fully assembled, provenance-tracked [`cubtera_model::Dimension`], with
//! a recursively resolved `meta.parent` chain) and
//! [`validate::ValidateUseCase`] (schema + dim-graph checks, one
//! dimension or the whole fleet). `plan`/`apply`/`fleet_status`/`drift`/
//! `explain` land in later phases (P4-P6) as this crate grows.

pub mod dim_graph_loader;
pub mod error;
pub mod ports;
pub mod resolve;
pub mod validate;

pub use dim_graph_loader::load_dim_graph;
pub use error::{AppError, AppResult};
pub use ports::InventoryPort;
pub use resolve::ResolveUseCase;
pub use validate::{DimensionValidation, FleetValidation, ValidateUseCase};
