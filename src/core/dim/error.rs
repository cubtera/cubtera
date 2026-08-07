use crate::error::{CubteraError, CubteraResult};
use thiserror::Error;

/// Dimension-specific error types
#[derive(Debug, Error)]
pub enum DimError {
    /// Dimension not found
    #[error("Dimension not found: {name}")]
    NotFound { name: String },
    
    /// Invalid dimension format
    #[error("Invalid dimension format: {input}. Expected format: 'type:name'")]
    InvalidFormat { input: String },
    
    /// Data source errors
    #[error("Data source error: {message}")]
    DataSource { message: String },
    
    /// Hierarchy errors
    #[error("Hierarchy error: {message}")]
    Hierarchy { message: String },
    
    /// File operation errors
    #[error("File operation error: {path}: {message}")]
    FileOperation { path: String, message: String },
    
    /// Validation errors
    #[error("Validation error: {field}: {message}")]
    Validation { field: String, message: String },
    
    /// Configuration errors
    #[error("Configuration error: {message}")]
    Configuration { message: String },
    
    /// Parent-child relationship errors
    #[error("Relationship error: {message}")]
    Relationship { message: String },
}

/// Result type for dimension operations
pub type DimResult<T> = CubteraResult<T>;

impl DimError {
    /// Create a dimension not found error
    pub fn not_found(name: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Dimension not found: {}", name.into())
        }
    }
    
    /// Create an invalid format error
    pub fn invalid_format(input: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Invalid dimension format: {}. Expected format: 'type:name'", input.into())
        }
    }
    
    /// Create a data source error
    pub fn data_source(message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Data source error: {}", message.into())
        }
    }
    
    /// Create a hierarchy error
    pub fn hierarchy(message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Hierarchy error: {}", message.into())
        }
    }
    
    /// Create a file operation error
    pub fn file_operation(path: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("File operation error: {}: {}", path.into(), message.into())
        }
    }
    
    /// Create a validation error
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Validation error: {}: {}", field.into(), message.into())
        }
    }
    
    /// Create a configuration error
    pub fn configuration(message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Configuration error: {}", message.into())
        }
    }
    
    /// Create a relationship error
    pub fn relationship(message: impl Into<String>) -> CubteraError {
        CubteraError::Dimension { 
            message: format!("Relationship error: {}", message.into())
        }
    }
}

/// Extension trait for dimension-specific error handling
pub trait DimResultExt<T> {
    /// Convert any error to a dimension not found error
    fn dim_not_found(self, name: &str) -> DimResult<T>;
    
    /// Convert any error to a data source error
    fn data_source_error(self, message: &str) -> DimResult<T>;
    
    /// Convert any error to a file operation error
    fn file_operation_error(self, path: &str) -> DimResult<T>;
    
    /// Convert any error to a validation error
    fn validation_error(self, field: &str) -> DimResult<T>;
    
    /// Convert any error to a hierarchy error
    fn hierarchy_error(self, message: &str) -> DimResult<T>;
}

impl<T, E: std::fmt::Display> DimResultExt<T> for std::result::Result<T, E> {
    fn dim_not_found(self, name: &str) -> DimResult<T> {
        self.map_err(|e| DimError::not_found(format!("{}: {}", name, e)))
    }
    
    fn data_source_error(self, message: &str) -> DimResult<T> {
        self.map_err(|e| DimError::data_source(format!("{}: {}", message, e)))
    }
    
    fn file_operation_error(self, path: &str) -> DimResult<T> {
        self.map_err(|e| DimError::file_operation(path, e.to_string()))
    }
    
    fn validation_error(self, field: &str) -> DimResult<T> {
        self.map_err(|e| DimError::validation(field, e.to_string()))
    }
    
    fn hierarchy_error(self, message: &str) -> DimResult<T> {
        self.map_err(|e| DimError::hierarchy(format!("{}: {}", message, e)))
    }
} 