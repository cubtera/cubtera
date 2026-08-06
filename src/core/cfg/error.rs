use crate::error::{CubteraError, CubteraResult};
use thiserror::Error;

/// Configuration-specific error types
#[derive(Debug, Error)]
pub enum ConfigError {
    /// Environment variable parsing errors
    #[error("Environment variable error: {message}")]
    EnvVar { message: String },
    
    /// Configuration file errors
    #[error("Config file error: {path}: {message}")]
    ConfigFile { path: String, message: String },
    
    /// Serialization/deserialization errors
    #[error("Serialization error: {format}: {message}")]
    Serialization { format: String, message: String },
    
    /// Path resolution errors
    #[error("Path error: {path}: {message}")]
    Path { path: String, message: String },
    
    /// Database connection errors
    #[error("Database connection error: {connection_string}: {message}")]
    Database { connection_string: String, message: String },
    
    /// Validation errors
    #[error("Validation error: {field}: {message}")]
    Validation { field: String, message: String },
}

/// Result type for configuration operations
pub type ConfigResult<T> = CubteraResult<T>;

impl ConfigError {
    /// Create an environment variable error
    pub fn env_var(message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Environment variable error: {}", message.into())
        }
    }
    
    /// Create a config file error
    pub fn config_file(path: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Config file error: {}: {}", path.into(), message.into())
        }
    }
    
    /// Create a serialization error
    pub fn serialization(format: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Serialization error: {}: {}", format.into(), message.into())
        }
    }
    
    /// Create a path error
    pub fn path_error(path: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Path error: {}: {}", path.into(), message.into())
        }
    }
    
    /// Create a database error
    pub fn database(connection_string: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Database connection error: {}: {}", connection_string.into(), message.into())
        }
    }
    
    /// Create a validation error
    pub fn validation(field: impl Into<String>, message: impl Into<String>) -> CubteraError {
        CubteraError::Config { 
            message: format!("Validation error: {}: {}", field.into(), message.into())
        }
    }
}

/// Extension trait for config-specific error handling
pub trait ConfigResultExt<T> {
    /// Convert any error to a config file error
    fn config_file_error(self, path: &str) -> ConfigResult<T>;
    
    /// Convert any error to an environment variable error
    fn env_var_error(self, var_name: &str) -> ConfigResult<T>;
    
    /// Convert any error to a serialization error
    fn serialization_error(self, format: &str) -> ConfigResult<T>;
    
    /// Convert any error to a path error
    fn path_error(self, path: &str) -> ConfigResult<T>;
    
    /// Convert any error to a database error
    fn database_error(self, connection_string: &str) -> ConfigResult<T>;
    
    /// Convert any error to a validation error
    fn validation_error(self, field: &str) -> ConfigResult<T>;
}

impl<T, E: std::fmt::Display> ConfigResultExt<T> for std::result::Result<T, E> {
    fn config_file_error(self, path: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::config_file(path, e.to_string()))
    }
    
    fn env_var_error(self, var_name: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::env_var(format!("{}: {}", var_name, e)))
    }
    
    fn serialization_error(self, format: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::serialization(format, e.to_string()))
    }
    
    fn path_error(self, path: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::path_error(path, e.to_string()))
    }
    
    fn database_error(self, connection_string: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::database(connection_string, e.to_string()))
    }
    
    fn validation_error(self, field: &str) -> ConfigResult<T> {
        self.map_err(|e| ConfigError::validation(field, e.to_string()))
    }
} 