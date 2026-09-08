//! Cubtera v3 model: typed inventory graph, gap-fill with provenance, and
//! content-addressed unit packages.
//!
//! Like `cubtera-kernel`, this crate is zero I/O and zero async - it is
//! the pure-logic layer `cubtera-app` (P3) orchestrates through ports.
//! Where v2's `cubtera-domain` mixed "one fixed `dimRelations` chain" with
//! "gap-fill happens, but nobody can tell you *where a value came from*",
//! this crate makes both of those first-class: [`DimGraph`] is a real
//! graph of named, typed edges (not one hardcoded chain), and
//! [`gap_fill_merge_with_provenance`] records which layer supplied each
//! field it didn't get from the dimension's own data.
//!
//! See docs/specs/2026-09-03-cubtera-v3-architecture.md ยง5.

mod access;
mod binding;
mod dim_graph;
mod dimension;
mod error;
mod ids;
mod instance;
mod lease;
mod manifest;
mod materialization;
mod output_set;
mod plan;
mod policy;
mod provenance;
mod revision;
mod run;
mod state_projection;
#[cfg(test)]
mod test_support;
mod unit;
mod unit_package;

pub use access::{AccessDecision, AccessPolicy, DimensionAccessContext};
pub use binding::{Binding, Literal, Path, Selector, SelectorContext};
pub use dim_graph::{DimEdge, DimGraph, DimTypeDef, GraphError, SchemaSpec};
pub use dimension::Dimension;
pub use error::ModelError;
pub use ids::{PlanId, RunId};
pub use instance::Instance;
pub use lease::Lease;
pub use manifest::{EnvVars, Files, InputSpec, Manifest, OutputsSpec, RunnerType, Spec};
pub use materialization::{MaterializationPlan, MaterializationStep};
pub use output_set::{OutputSet, OutputValue, SecretRef, StaleConsumer};
pub use plan::{Plan, ResolutionManifest};
pub use policy::{context_for, Effect, Policy, PolicyDecision, PolicyRule};
pub use provenance::{
    field_provenance_for, gap_fill_merge_with_provenance, FieldProvenance, ProvenanceSource,
};
pub use revision::Revision;
pub use run::{Run, RunFilter, RunOp, RunPatch, RunStatus};
pub use state_projection::project_state_key;
pub use unit::{IncludeEntry, Unit};
pub use unit_package::{PinnedModule, UnitPackage};

pub type ModelResult<T> = Result<T, ModelError>;
