//! Cubtera v3 application layer: use cases and their ports.
//!
//! Depends only on `cubtera-model`/`cubtera-kernel` (see the crate table in
//! docs/specs/2026-09-03-cubtera-v3-architecture.md section 3) - every I/O
//! boundary a use case needs is a trait in [`ports`], implemented by an
//! adapter in a leaf crate (for P3, a thin bridge onto the existing
//! v2 `cubtera-persistence` FS adapter, wired up in `crates/cubtera`).
//!
//! P3 shipped [`resolve::ResolveUseCase`] (raw record -> fully assembled,
//! provenance-tracked [`cubtera_model::Dimension`], with a recursively
//! resolved `meta.parent` chain) and [`validate::ValidateUseCase`] (schema
//! + dim-graph checks, one dimension or the whole fleet).
//!
//! P4 adds [`run::RunUseCase`] (`plan`/`apply --plan`/`explain run`),
//! which needs real persistence and content-addressing - so, unlike P3,
//! this crate now also depends on `cubtera-store` (the `Store` port) and
//! `cubtera-source` (the `SourceRepo` port), both of which only depend on
//! `kernel`/`model` themselves, so this doesn't introduce a cycle. `fleet
//! status`/`drift`/bindings still land in P5-P6.

//!
//! P5 adds [`bindings::BindingUseCase`] (`expand`/`status`/
//! `group_by_wave`): desired state over the inventory, drift against
//! `Store`, and wave batching. It reuses `run::compute_package` (the exact
//! "hash this unit's files right now" computation `plan`/`apply` already
//! agree on) rather than inventing a second one.

pub mod bindings;
pub mod dim_graph_loader;
pub mod error;
pub mod ports;
pub mod resolve;
pub mod run;
pub mod validate;

pub use bindings::{group_by_wave, BindingUseCase, DriftState, InstanceDrift};
pub use dim_graph_loader::load_dim_graph;
pub use error::{AppError, AppResult};
pub use ports::{Clock, InventoryPort, SystemClock};
pub use resolve::ResolveUseCase;
pub use run::{ApplyRequest, PlanRequest, RunUseCase};
pub use validate::{DimensionValidation, FleetValidation, ValidateUseCase};
