use thiserror::Error;

/// Centralized error type for the entire Cubtera application
/// 
/// This enum provides a unified error handling approach while maintaining
/// module-specific error contexts. Each variant represents a different
/// domain/module within the application.
#[derive(Debug, Error)]
pub enum CubteraError {
    /// Tools module errors (file operations, JSON, Git, etc.)
    #[error("Tools error: {0}")]
    Tools(#[from] crate::tools::ToolsError),
    
    /// Configuration module errors
    #[error("Configuration error: {message}")]
    Config { message: String },
    
    /// Dimension module errors
    #[error("Dimension error: {message}")]
    Dimension { message: String },
    
    /// Unit module errors
    #[error("Unit error: {message}")]
    Unit { message: String },
    
    /// Runner module errors
    #[error("Runner error: {runner_type}: {message}")]
    Runner { runner_type: String, message: String },
    
    /// Image management errors
    #[error("Image management error: {message}")]
    Image { message: String },
    
    /// Deployment log errors
    #[error("Deployment log error: {message}")]
    DeploymentLog { message: String },
    
    /// CLI command errors
    #[error("CLI error: {command}: {message}")]
    Cli { command: String, message: String },
    
    /// API endpoint errors
    #[error("API error: {endpoint}: {message}")]
    Api { endpoint: String, message: String },
    
    /// Validation errors (cross-module)
    #[error("Validation error: {field}: {message}")]
    Validation { field: String, message: String },
    
    /// External dependency errors
    #[error("External dependency error: {dependency}: {message}")]
    External { dependency: String, message: String },
    
    /// Critical system errors that should cause immediate exit
    #[error("Critical system error: {message}")]
    Critical { message: String },
}

/// Result type alias for the entire Cubtera application
/// 
/// Note: This is intentionally named CubteraResult to avoid conflicts
/// with existing Result types in the codebase during migration.
pub type CubteraResult<T> = std::result::Result<T, CubteraError>;

impl CubteraError {
    /// Create a configuration error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::Config { message: message.into() }
    }
    
    /// Create a dimension error
    pub fn dimension_error(message: impl Into<String>) -> Self {
        Self::Dimension { message: message.into() }
    }
    
    /// Create a unit error
    pub fn unit_error(message: impl Into<String>) -> Self {
        Self::Unit { message: message.into() }
    }
    
    /// Create a runner error
    pub fn runner_error(runner_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Runner { 
            runner_type: runner_type.into(), 
            message: message.into() 
        }
    }
    
    /// Create an image management error
    pub fn image_error(message: impl Into<String>) -> Self {
        Self::Image { message: message.into() }
    }
    
    /// Create a deployment log error
    pub fn deployment_log_error(message: impl Into<String>) -> Self {
        Self::DeploymentLog { message: message.into() }
    }
    
    /// Create a CLI error
    pub fn cli_error(command: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Cli { 
            command: command.into(), 
            message: message.into() 
        }
    }
    
    /// Create an API error
    pub fn api_error(endpoint: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Api { 
            endpoint: endpoint.into(), 
            message: message.into() 
        }
    }
    
    /// Create a validation error
    pub fn validation_error(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self::Validation { 
            field: field.into(), 
            message: message.into() 
        }
    }
    
    /// Create an external dependency error
    pub fn external_error(dependency: impl Into<String>, message: impl Into<String>) -> Self {
        Self::External { 
            dependency: dependency.into(), 
            message: message.into() 
        }
    }
    
    /// Create a critical system error
    pub fn critical_error(message: impl Into<String>) -> Self {
        Self::Critical { message: message.into() }
    }
    
    /// Check if this error is critical and should cause immediate exit
    pub fn is_critical(&self) -> bool {
        matches!(self, CubteraError::Critical { .. })
    }
    
    /// Get the error category for logging/metrics
    pub fn category(&self) -> &'static str {
        match self {
            CubteraError::Tools(_) => "tools",
            CubteraError::Config { .. } => "config",
            CubteraError::Dimension { .. } => "dimension",
            CubteraError::Unit { .. } => "unit",
            CubteraError::Runner { .. } => "runner",
            CubteraError::Image { .. } => "image",
            CubteraError::DeploymentLog { .. } => "deployment_log",
            CubteraError::Cli { .. } => "cli",
            CubteraError::Api { .. } => "api",
            CubteraError::Validation { .. } => "validation",
            CubteraError::External { .. } => "external",
            CubteraError::Critical { .. } => "critical",
        }
    }
    
    /// Get error severity level
    pub fn severity(&self) -> ErrorSeverity {
        match self {
            CubteraError::Critical { .. } => ErrorSeverity::Critical,
            CubteraError::Config { .. } => ErrorSeverity::High,
            CubteraError::Validation { .. } => ErrorSeverity::High,
            CubteraError::Runner { .. } => ErrorSeverity::Medium,
            CubteraError::Unit { .. } => ErrorSeverity::Medium,
            CubteraError::Dimension { .. } => ErrorSeverity::Medium,
            CubteraError::Tools(_) => ErrorSeverity::Low,
            CubteraError::Image { .. } => ErrorSeverity::Low,
            CubteraError::DeploymentLog { .. } => ErrorSeverity::Low,
            CubteraError::Cli { .. } => ErrorSeverity::Medium,
            CubteraError::Api { .. } => ErrorSeverity::Medium,
            CubteraError::External { .. } => ErrorSeverity::Medium,
        }
    }
}

/// Error severity levels for logging and handling decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorSeverity {
    Low,
    Medium,
    High,
    Critical,
}

impl ErrorSeverity {
    /// Should this error cause the application to exit?
    pub fn should_exit(&self) -> bool {
        matches!(self, ErrorSeverity::Critical)
    }
    
    /// Get the log level for this severity
    pub fn log_level(&self) -> log::Level {
        match self {
            ErrorSeverity::Low => log::Level::Warn,
            ErrorSeverity::Medium => log::Level::Error,
            ErrorSeverity::High => log::Level::Error,
            ErrorSeverity::Critical => log::Level::Error,
        }
    }
}

/// Automatic conversions from standard library errors
impl From<std::io::Error> for CubteraError {
    fn from(err: std::io::Error) -> Self {
        CubteraError::external_error("std::io", err.to_string())
    }
}

impl From<serde_json::Error> for CubteraError {
    fn from(err: serde_json::Error) -> Self {
        CubteraError::external_error("serde_json", err.to_string())
    }
}

impl From<git2::Error> for CubteraError {
    fn from(err: git2::Error) -> Self {
        CubteraError::external_error("git2", err.to_string())
    }
}

impl From<mongodb::error::Error> for CubteraError {
    fn from(err: mongodb::error::Error) -> Self {
        CubteraError::external_error("mongodb", err.to_string())
    }
}

impl From<mongodb::bson::de::Error> for CubteraError {
    fn from(err: mongodb::bson::de::Error) -> Self {
        CubteraError::external_error("mongodb::bson::de", err.to_string())
    }
}

impl From<mongodb::bson::ser::Error> for CubteraError {
    fn from(err: mongodb::bson::ser::Error) -> Self {
        CubteraError::external_error("mongodb::bson::ser", err.to_string())
    }
}

impl From<Box<dyn std::error::Error>> for CubteraError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        CubteraError::external_error("boxed_error", err.to_string())
    }
}

/// Extension trait for Result types to provide Cubtera-specific error handling
pub trait CubteraResultExt<T> {
    /// Add context to any error and convert it to CubteraError
    fn with_context(self, context: &str) -> CubteraResult<T>;
    
    /// Convert to a specific Cubtera error type
    fn to_config_error(self, message: &str) -> CubteraResult<T>;
    fn to_dimension_error(self, message: &str) -> CubteraResult<T>;
    fn to_unit_error(self, message: &str) -> CubteraResult<T>;
    fn to_runner_error(self, runner_type: &str, message: &str) -> CubteraResult<T>;
    fn to_cli_error(self, command: &str, message: &str) -> CubteraResult<T>;
    fn to_api_error(self, endpoint: &str, message: &str) -> CubteraResult<T>;
}

impl<T, E: std::fmt::Display> CubteraResultExt<T> for std::result::Result<T, E> {
    fn with_context(self, context: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::external_error("context", format!("{}: {}", context, e)))
    }
    
    fn to_config_error(self, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::config_error(format!("{}: {}", message, e)))
    }
    
    fn to_dimension_error(self, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::dimension_error(format!("{}: {}", message, e)))
    }
    
    fn to_unit_error(self, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::unit_error(format!("{}: {}", message, e)))
    }
    
    fn to_runner_error(self, runner_type: &str, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::runner_error(runner_type, format!("{}: {}", message, e)))
    }
    
    fn to_cli_error(self, command: &str, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::cli_error(command, format!("{}: {}", message, e)))
    }
    
    fn to_api_error(self, endpoint: &str, message: &str) -> CubteraResult<T> {
        self.map_err(|e| CubteraError::api_error(endpoint, format!("{}: {}", message, e)))
    }
} 