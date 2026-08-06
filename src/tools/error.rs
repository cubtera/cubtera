use thiserror::Error;

/// Centralized error type for all tools operations
#[derive(Debug, Error)]
pub enum ToolsError {
    #[error("Git error: {0}")]
    Git(#[from] git2::Error),
    
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    
    #[error("JSON schema validation error: {0}")]
    JsonSchema(#[from] jsonschema::ValidationError<'static>),
    
    #[error("MongoDB error: {0}")]
    MongoDB(#[from] mongodb::error::Error),
    
    #[error("Path error: {message}")]
    Path { message: String },
    
    #[error("Process error: {message}")]
    Process { message: String },
    
    #[error("Validation error: {message}")]
    Validation { message: String },
    
    #[error("Configuration error: {message}")]
    Config { message: String },
    
    #[error("Crypto error: {message}")]
    Crypto { message: String },
    
    #[error("Database connection error: {message}")]
    Database { message: String },
    
    #[error("File not found: {path}")]
    FileNotFound { path: String },
    
    #[error("Permission denied: {path}")]
    PermissionDenied { path: String },
    
    #[error("Invalid input: {message}")]
    InvalidInput { message: String },
    
    #[error("Operation failed: {message}")]
    OperationFailed { message: String },
}

/// Result type alias for tools operations
pub type Result<T> = std::result::Result<T, ToolsError>;

impl ToolsError {
    /// Create a new path error
    pub fn path_error(message: impl Into<String>) -> Self {
        Self::Path { message: message.into() }
    }
    
    /// Create a new process error
    pub fn process_error(message: impl Into<String>) -> Self {
        Self::Process { message: message.into() }
    }
    
    /// Create a new validation error
    pub fn validation_error(message: impl Into<String>) -> Self {
        Self::Validation { message: message.into() }
    }
    
    /// Create a new config error
    pub fn config_error(message: impl Into<String>) -> Self {
        Self::Config { message: message.into() }
    }
    
    /// Create a new crypto error
    pub fn crypto_error(message: impl Into<String>) -> Self {
        Self::Crypto { message: message.into() }
    }
    
    /// Create a new database error
    pub fn database_error(message: impl Into<String>) -> Self {
        Self::Database { message: message.into() }
    }
    
    /// Create a new file not found error
    pub fn file_not_found(path: impl Into<String>) -> Self {
        Self::FileNotFound { path: path.into() }
    }
    
    /// Create a new permission denied error
    pub fn permission_denied(path: impl Into<String>) -> Self {
        Self::PermissionDenied { path: path.into() }
    }
    
    /// Create a new invalid input error
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::InvalidInput { message: message.into() }
    }
    
    /// Create a new operation failed error
    pub fn operation_failed(message: impl Into<String>) -> Self {
        Self::OperationFailed { message: message.into() }
    }
} 