use crate::tools::error::{Result as ToolsResult, ToolsError};
use crate::error::{CubteraError, CubteraResult, ErrorSeverity};
use log::{error, warn};

/// Legacy compatibility function for exit_with_error
/// 
/// This function maintains the same behavior as the old exit_with_error
/// but provides a clear migration path to proper error handling.
/// 
/// # Arguments
/// * `message` - Error message to log and exit with
/// 
/// # Examples
/// ```
/// use cubtera::tools::compat::exit_with_error;
/// 
/// // This will log the error and exit the process
/// exit_with_error("Critical configuration error".to_string());
/// ```
pub fn exit_with_error(message: String) -> ! {
    error!(target: "", "{}", message);
    std::process::exit(1);
}

/// Extension trait for Result types to provide legacy compatibility methods
/// 
/// This trait provides migration helpers that maintain similar behavior
/// to the old unwrap_or_exit and check_with_warn patterns while encouraging
/// proper error handling.
pub trait LegacyCompat<T> {
    /// Unwrap the result or exit the process with an error message
    /// 
    /// This maintains the old unwrap_or_exit behavior but should be used
    /// sparingly and only in CLI entry points.
    /// 
    /// # Arguments
    /// * `context` - Context message for the error
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::LegacyCompat;
    /// 
    /// let config = read_config()
    ///     .unwrap_or_exit_with_log("Failed to read configuration");
    /// ```
    fn unwrap_or_exit_with_log(self, context: &str) -> T;
    
    /// Log a warning and return a default value if the result is an error
    /// 
    /// This provides similar behavior to check_with_warn but returns a default
    /// value instead of continuing with None.
    /// 
    /// # Arguments
    /// * `warning` - Warning message to log
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::LegacyCompat;
    /// 
    /// let config = read_optional_config()
    ///     .warn_and_default("Optional config not found, using defaults");
    /// ```
    fn warn_and_default(self, warning: &str) -> T where T: Default;
    
    /// Log a warning and return None if the result is an error
    /// 
    /// This provides similar behavior to check_with_warn for optional operations.
    /// 
    /// # Arguments
    /// * `warning` - Warning message to log
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::LegacyCompat;
    /// 
    /// if let Some(cache) = load_cache()
    ///     .warn_and_continue("Cache file corrupted, proceeding without cache") {
    ///     // Use cache
    /// }
    /// ```
    fn warn_and_continue(self, warning: &str) -> Option<T>;
    
    /// Convert any error to ToolsError with context
    /// 
    /// This helps migrate from unwrap_or_exit patterns to proper error propagation.
    /// 
    /// # Arguments
    /// * `context` - Context message for the error
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::LegacyCompat;
    /// 
    /// fn load_config() -> ToolsResult<Config> {
    ///     let content = std::fs::read_to_string("config.toml")
    ///         .with_context("Failed to read config file")?;
    ///     // ... rest of function
    ///     Ok(config)
    /// }
    /// ```
    fn with_context(self, context: &str) -> ToolsResult<T>;
}

impl<T, E: std::fmt::Display> LegacyCompat<T> for std::result::Result<T, E> {
    fn unwrap_or_exit_with_log(self, context: &str) -> T {
        match self {
            Ok(value) => value,
            Err(e) => {
                error!(target: "", "{}: {}", context, e);
                std::process::exit(1);
            }
        }
    }
    
    fn warn_and_default(self, warning: &str) -> T where T: Default {
        match self {
            Ok(value) => value,
            Err(e) => {
                warn!(target: "", "{}: {}", warning, e);
                T::default()
            }
        }
    }
    
    fn warn_and_continue(self, warning: &str) -> Option<T> {
        match self {
            Ok(value) => Some(value),
            Err(e) => {
                warn!(target: "", "{}: {}", warning, e);
                None
            }
        }
    }
    
    fn with_context(self, context: &str) -> ToolsResult<T> {
        self.map_err(|e| ToolsError::operation_failed(format!("{}: {}", context, e)))
    }
}

/// Extension trait for CubteraError results to provide smart error handling
/// 
/// This trait provides intelligent error handling based on error severity.
pub trait CubteraCompat<T> {
    /// Handle error based on its severity
    /// 
    /// - Critical errors: Exit immediately
    /// - High severity: Log error and exit
    /// - Medium severity: Log error and return default
    /// - Low severity: Log warning and return default
    fn handle_by_severity(self) -> T where T: Default;
    
    /// Handle error with custom behavior per severity
    fn handle_with_strategy<F>(self, strategy: F) -> T 
    where 
        F: Fn(ErrorSeverity, &CubteraError) -> T;
    
    /// Convert to tools error for backward compatibility
    fn to_tools_error(self) -> ToolsResult<T>;
}

impl<T> CubteraCompat<T> for CubteraResult<T> {
    fn handle_by_severity(self) -> T where T: Default {
        match self {
            Ok(value) => value,
            Err(e) => {
                let severity = e.severity();
                match severity {
                    ErrorSeverity::Critical => {
                        error!(target: "", "Critical error: {}", e);
                        std::process::exit(1);
                    }
                    ErrorSeverity::High => {
                        error!(target: "", "High severity error: {}", e);
                        std::process::exit(1);
                    }
                    ErrorSeverity::Medium => {
                        error!(target: "", "Medium severity error: {}", e);
                        T::default()
                    }
                    ErrorSeverity::Low => {
                        warn!(target: "", "Low severity error: {}", e);
                        T::default()
                    }
                }
            }
        }
    }
    
    fn handle_with_strategy<F>(self, strategy: F) -> T 
    where 
        F: Fn(ErrorSeverity, &CubteraError) -> T
    {
        match self {
            Ok(value) => value,
            Err(e) => {
                let severity = e.severity();
                strategy(severity, &e)
            }
        }
    }
    
    fn to_tools_error(self) -> ToolsResult<T> {
        self.map_err(|e| match e {
            CubteraError::Tools(tools_err) => tools_err,
            other => ToolsError::operation_failed(other.to_string()),
        })
    }
}

/// Extension trait specifically for Option types
/// 
/// Provides legacy compatibility for Option unwrapping patterns.
pub trait OptionCompat<T> {
    /// Unwrap the option or exit with an error message
    /// 
    /// This maintains the old unwrap_or_exit behavior for Option types.
    /// 
    /// # Arguments
    /// * `error_message` - Error message to log and exit with
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::OptionCompat;
    /// 
    /// let value = some_option
    ///     .unwrap_or_exit_with_log("Required value not found");
    /// ```
    fn unwrap_or_exit_with_log(self, error_message: &str) -> T;
    
    /// Convert None to a ToolsError with context
    /// 
    /// # Arguments
    /// * `context` - Context message for the error
    /// 
    /// # Examples
    /// ```
    /// use cubtera::tools::compat::OptionCompat;
    /// 
    /// let value = some_option
    ///     .ok_or_context("Required configuration value missing")?;
    /// ```
    fn ok_or_context(self, context: &str) -> ToolsResult<T>;
    
    /// Convert None to a CubteraError with context
    fn ok_or_cubtera_context(self, context: &str) -> CubteraResult<T>;
}

impl<T> OptionCompat<T> for Option<T> {
    fn unwrap_or_exit_with_log(self, error_message: &str) -> T {
        match self {
            Some(value) => value,
            None => {
                error!(target: "", "{}", error_message);
                std::process::exit(1);
            }
        }
    }
    
    fn ok_or_context(self, context: &str) -> ToolsResult<T> {
        self.ok_or_else(|| ToolsError::operation_failed(context.to_string()))
    }
    
    fn ok_or_cubtera_context(self, context: &str) -> CubteraResult<T> {
        self.ok_or_else(|| CubteraError::validation_error("option", context))
    }
}

/// Macro for easy migration from unwrap_or_exit patterns
/// 
/// This macro provides a convenient way to migrate from the old patterns
/// while maintaining similar syntax.
/// 
/// # Examples
/// ```
/// use cubtera::tools::compat::unwrap_or_exit;
/// 
/// let config = unwrap_or_exit!(read_config(), "Failed to read config");
/// ```
#[macro_export]
macro_rules! unwrap_or_exit {
    ($result:expr, $msg:expr) => {
        $crate::tools::compat::LegacyCompat::unwrap_or_exit_with_log($result, $msg)
    };
}

/// Macro for warning and continuing with default values
/// 
/// # Examples
/// ```
/// use cubtera::tools::compat::warn_and_default;
/// 
/// let config = warn_and_default!(read_optional_config(), "Using default config");
/// ```
#[macro_export]
macro_rules! warn_and_default {
    ($result:expr, $msg:expr) => {
        $crate::tools::compat::LegacyCompat::warn_and_default($result, $msg)
    };
}

/// Macro for smart error handling based on severity
/// 
/// # Examples
/// ```
/// use cubtera::tools::compat::handle_smart;
/// 
/// let config = handle_smart!(load_config());
/// ```
#[macro_export]
macro_rules! handle_smart {
    ($result:expr) => {
        $crate::tools::compat::CubteraCompat::handle_by_severity($result)
    };
}

/// Migration guide and examples
/// 
/// This module provides examples of how to migrate from old patterns to new ones.
pub mod migration_examples {
    use super::*;
    
    /// Example of migrating from exit_with_error
    /// 
    /// Old pattern:
    /// ```ignore
    /// if config.is_none() {
    ///     exit_with_error("Configuration is required".to_string());
    /// }
    /// ```
    /// 
    /// New pattern (library code):
    /// ```ignore
    /// fn load_config() -> CubteraResult<Config> {
    ///     config.ok_or_else(|| CubteraError::config_error("Configuration is required"))
    /// }
    /// ```
    /// 
    /// New pattern (CLI code):
    /// ```ignore
    /// let config = load_config()
    ///     .unwrap_or_exit_with_log("Failed to load configuration");
    /// ```
    pub fn migration_example_exit_with_error() {
        // Implementation examples would go here
    }
    
    /// Example of migrating from unwrap_or_exit
    /// 
    /// Old pattern:
    /// ```ignore
    /// let data = read_file(path)
    ///     .unwrap_or_exit("Failed to read file".to_string());
    /// ```
    /// 
    /// New pattern (library code):
    /// ```ignore
    /// fn process_file(path: &str) -> CubteraResult<ProcessedData> {
    ///     let data = read_file(path)
    ///         .to_config_error("Failed to read file")?;
    ///     // ... process data
    ///     Ok(processed_data)
    /// }
    /// ```
    /// 
    /// New pattern (CLI code):
    /// ```ignore
    /// let processed = process_file(path)
    ///     .handle_by_severity();
    /// ```
    pub fn migration_example_unwrap_or_exit() {
        // Implementation examples would go here
    }
    
    /// Example of migrating from check_with_warn
    /// 
    /// Old pattern:
    /// ```ignore
    /// let cache = load_cache()
    ///     .check_with_warn("Cache not available, proceeding without cache");
    /// ```
    /// 
    /// New pattern:
    /// ```ignore
    /// let cache = load_cache()
    ///     .warn_and_continue("Cache not available, proceeding without cache");
    /// ```
    pub fn migration_example_check_with_warn() {
        // Implementation examples would go here
    }
} 