//! Application-layer errors.
//!
//! Distinct from `cubtera_core::error::AppError` (v2) - this crate cannot
//! depend on `cubtera-core` (see the crate table in
//! docs/specs/2026-09-03-cubtera-v3-architecture.md section 3: `cubtera-app`
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

    /// `AssembleUseCase` denied a unit run per its `allowList`/`denyList`/
    /// `affinityTags` (`cubtera_model::AccessPolicy::evaluate`) - a real,
    /// non-zero outcome (CLI exit code 3), never a silent `exit(0)`.
    #[error("access denied: {0}")]
    AccessDenied(String),
}

impl From<cubtera_store::StoreError> for AppError {
    fn from(e: cubtera_store::StoreError) -> Self {
        AppError::Backend(e.to_string())
    }
}

impl From<cubtera_source::SourceError> for AppError {
    fn from(e: cubtera_source::SourceError) -> Self {
        AppError::Backend(e.to_string())
    }
}

impl From<cubtera_kernel::KernelError> for AppError {
    fn from(e: cubtera_kernel::KernelError) -> Self {
        AppError::Validation(e.to_string())
    }
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

    pub fn access_denied(msg: impl Into<String>) -> Self {
        Self::AccessDenied(msg.into())
    }
}
