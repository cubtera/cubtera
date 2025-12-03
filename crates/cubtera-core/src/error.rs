//! Application layer errors

use cubtera_domain::DomainError;
use thiserror::Error;

/// Application layer result type
pub type AppResult<T> = Result<T, AppError>;

/// Application layer errors
#[derive(Debug, Error)]
pub enum AppError {
    /// Domain error
    #[error("{0}")]
    Domain(#[from] DomainError),

    /// Repository error
    #[error("Repository error: {0}")]
    Repository(String),

    /// Configuration error
    #[error("Configuration error: {0}")]
    Config(String),

    /// Runner error
    #[error("Runner error: {0}")]
    Runner(String),

    /// IO error
    #[error("IO error: {0}")]
    Io(String),

    /// Not found
    #[error("{entity} not found: {id}")]
    NotFound { entity: &'static str, id: String },

    /// Validation error
    #[error("Validation error: {0}")]
    Validation(String),
}

impl AppError {
    /// Create a repository error
    pub fn repository(msg: impl Into<String>) -> Self {
        Self::Repository(msg.into())
    }

    /// Create a config error
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config(msg.into())
    }

    /// Create a runner error
    pub fn runner(msg: impl Into<String>) -> Self {
        Self::Runner(msg.into())
    }

    /// Create an IO error
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }

    /// Create a not found error
    pub fn not_found(entity: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            entity,
            id: id.into(),
        }
    }

    /// Create a validation error
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }
}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

