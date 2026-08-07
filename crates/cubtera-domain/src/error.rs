//! Domain errors
//!
//! All domain-level errors that can occur in business logic.

use std::fmt;

/// Result type for domain operations
pub type DomainResult<T> = Result<T, DomainError>;

/// Domain-level errors
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DomainError {
    /// Dimension not found
    DimensionNotFound {
        dim_type: String,
        name: String,
    },
    /// Invalid dimension format
    InvalidDimensionFormat {
        input: String,
        expected: &'static str,
    },
    /// Invalid manifest
    InvalidManifest {
        reason: String,
    },
    /// Missing required dimension
    MissingRequiredDimension {
        dim_type: String,
    },
    /// Dimension not allowed
    DimensionNotAllowed {
        dim_type: String,
        name: String,
        reason: String,
    },
    /// Invalid hierarchy
    InvalidHierarchy {
        reason: String,
    },
    /// Unit not found
    UnitNotFound {
        name: String,
    },
    /// Runner error
    RunnerError {
        reason: String,
    },
    /// IO/File operation error
    IoError {
        reason: String,
    },
}

impl DomainError {
    /// Create a new IO error
    pub fn io(reason: impl Into<String>) -> Self {
        Self::IoError {
            reason: reason.into(),
        }
    }

    /// Create a runner error
    pub fn runner(reason: impl Into<String>) -> Self {
        Self::RunnerError {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for DomainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DimensionNotFound { dim_type, name } => {
                write!(f, "Dimension not found: {}:{}", dim_type, name)
            }
            Self::InvalidDimensionFormat { input, expected } => {
                write!(
                    f,
                    "Invalid dimension format: '{}', expected: {}",
                    input, expected
                )
            }
            Self::InvalidManifest { reason } => {
                write!(f, "Invalid manifest: {}", reason)
            }
            Self::MissingRequiredDimension { dim_type } => {
                write!(f, "Missing required dimension: {}", dim_type)
            }
            Self::DimensionNotAllowed { dim_type, name, reason } => {
                write!(
                    f,
                    "Dimension not allowed: {}:{} - {}",
                    dim_type, name, reason
                )
            }
            Self::InvalidHierarchy { reason } => {
                write!(f, "Invalid hierarchy: {}", reason)
            }
            Self::UnitNotFound { name } => {
                write!(f, "Unit not found: {}", name)
            }
            Self::RunnerError { reason } => {
                write!(f, "Runner error: {}", reason)
            }
            Self::IoError { reason } => {
                write!(f, "IO error: {}", reason)
            }
        }
    }
}

impl std::error::Error for DomainError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dimension_not_found_display() {
        let err = DomainError::DimensionNotFound {
            dim_type: "env".to_string(),
            name: "prod".to_string(),
        };
        assert_eq!(err.to_string(), "Dimension not found: env:prod");
    }

    #[test]
    fn test_invalid_dimension_format_display() {
        let err = DomainError::InvalidDimensionFormat {
            input: "invalid".to_string(),
            expected: "<dim_type>:<dim_name>",
        };
        assert!(err.to_string().contains("invalid"));
        assert!(err.to_string().contains("<dim_type>:<dim_name>"));
    }
}

