//! Application-layer errors.
//!
//! Distinct from `cubtera_core::error::AppError` (v2) - this crate cannot
//! depend on `cubtera-core` (see the crate table in
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง3: `cubtera-app`
//! depends on `model, kernel` only), so it defines its own boundary error
//! type. `crates/cubtera/src/error.rs` maps both to CLI exit codes.

use cubtera_model::ModelError;
use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    /// The requested dimension/type doesn't exist in the inventory.
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    /// Malformed identifiers or other input the caller must fix.
    #[error("validation failed: {0}")]
    Validation(String),

    /// The inventory's dim-type graph is inconsistent (unknown edge
    /// target, gap-fill cycle - see [`cubtera_model::DimGraph::validate`])
    /// or a dimension failed its type's JSON schema.
    #[error("{0}")]
    Model(#[from] ModelError),

    /// The port adapter behind an `InventoryPort` (FS, SQLite, ...) failed.
    #[error("backend error: {0}")]
    Backend(String),
}

impl AppError {
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }

    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    pub fn backend(msg: impl Into<String>) -> Self {
        Self::Backend(msg.into())
    }
}
