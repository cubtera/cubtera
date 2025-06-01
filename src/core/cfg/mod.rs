pub mod error;
pub mod loader;
pub mod config;

// Re-export main types for convenience
pub use config::CubteraConfig;
pub use error::{ConfigError, ConfigResult, ConfigResultExt};
pub use loader::ConfigLoader;

use crate::tools::{CubteraCompat, exit_with_error};
use once_cell::sync::Lazy;

/// Global configuration instance
/// 
/// This uses the new error-safe loading mechanism with smart error handling.
/// Critical configuration errors will cause the application to exit,
/// while non-critical errors will log warnings and use defaults.
pub static GLOBAL_CFG: Lazy<CubteraConfig> = Lazy::new(|| {
    match CubteraConfig::load() {
        Ok(config) => config,
        Err(e) => {
            // For critical configuration errors, exit the application
            exit_with_error(format!("Failed to load configuration: {}", e));
        }
    }
});

/// Initialize global configuration with custom error handling
/// 
/// This function allows for custom initialization of the global configuration
/// with specific error handling strategies.
pub fn init_global_config() -> ConfigResult<()> {
    // Force initialization of the global config
    Lazy::force(&GLOBAL_CFG);
    Ok(())
}

/// Get a reference to the global configuration
/// 
/// This is a safe way to access the global configuration that has been
/// initialized with proper error handling.
pub fn get_global_config() -> &'static CubteraConfig {
    &GLOBAL_CFG
}

/// Load configuration with custom settings
/// 
/// This function provides a way to load configuration with custom
/// environment prefix or config file path for testing or special use cases.
pub fn load_custom_config(
    env_prefix: Option<&str>,
    config_path: Option<&str>,
) -> ConfigResult<CubteraConfig> {
    let mut loader = ConfigLoader::new();
    
    if let Some(prefix) = env_prefix {
        loader = loader.with_env_prefix(prefix);
    }
    
    if let Some(path) = config_path {
        loader = loader.with_default_config_path(path);
    }
    
    CubteraConfig::load_with_loader(loader)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    
    #[test]
    fn test_global_config_access() {
        let config = get_global_config();
        assert!(!config.org.is_empty());
    }
    
    #[test]
    fn test_load_custom_config() {
        let result = load_custom_config(Some("TEST"), Some("/nonexistent/config.toml"));
        // Should succeed with defaults even if config file doesn't exist
        assert!(result.is_ok());
    }
    
    #[test]
    fn test_init_global_config() {
        let result = init_global_config();
        assert!(result.is_ok());
    }
} 