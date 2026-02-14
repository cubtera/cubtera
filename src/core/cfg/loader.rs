use super::error::{ConfigError, ConfigResult, ConfigResultExt};
use crate::tools::convert_path_to_absolute;
use log::{warn, debug};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use yansi::Paint;

/// Configuration loader responsible for reading config from various sources
pub struct ConfigLoader {
    /// Environment variable prefix
    env_prefix: String,
    /// Default config file path
    default_config_path: String,
}

impl Default for ConfigLoader {
    fn default() -> Self {
        Self {
            env_prefix: "CUBTERA".to_string(),
            default_config_path: "~/.cubtera/config.toml".to_string(),
        }
    }
}

impl ConfigLoader {
    /// Create a new config loader
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Set custom environment variable prefix
    pub fn with_env_prefix(mut self, prefix: impl Into<String>) -> Self {
        self.env_prefix = prefix.into();
        self
    }
    
    /// Set custom default config file path
    pub fn with_default_config_path(mut self, path: impl Into<String>) -> Self {
        self.default_config_path = path.into();
        self
    }
    
    /// Load environment variables with the configured prefix
    pub fn load_env_vars(&self) -> ConfigResult<HashMap<String, Value>> {
        debug!("Loading environment variables with prefix: {}", self.env_prefix.yellow());
        
        config::Config::builder()
            .add_source(
                config::Environment::with_prefix(&self.env_prefix)
                    .ignore_empty(true)
                    .prefix_separator("_")
                    .separator("__"),
            )
            .build()
            .env_var_error(&format!("{}_*", self.env_prefix))?
            .try_deserialize::<HashMap<String, Value>>()
            .env_var_error(&format!("{}_* deserialization", self.env_prefix))
    }
    
    /// Determine config file path from environment variables or use default
    pub fn determine_config_path(&self, env_vars: &HashMap<String, Value>) -> ConfigResult<String> {
        let config_path = env_vars
            .get("config")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .map(|s| {
                convert_path_to_absolute(s.clone())
                    .unwrap_or_else(|_| {
                        warn!("Failed to convert config path to absolute: {}", s);
                        s
                    })
            })
            .unwrap_or_else(|| {
                warn!(
                    target: "cfg", 
                    "{}_CONFIG is not provided. Using: {}", 
                    self.env_prefix,
                    self.default_config_path.blue()
                );
                
                convert_path_to_absolute(self.default_config_path.clone())
                    .unwrap_or_else(|_| {
                        warn!("Failed to convert default config path to absolute: {}", self.default_config_path);
                        self.default_config_path.clone()
                    })
            });
            
        Ok(config_path)
    }
    
    /// Load configuration from file (TOML format)
    pub fn load_config_file(&self, path: &str) -> ConfigResult<HashMap<String, HashMap<String, Value>>> {
        debug!("Loading config file: {}", path.blue());
        
        // Try to load the config file, but don't fail if it doesn't exist
        let config_result = config::Config::builder()
            .add_source(config::File::from(Path::new(path)))
            .build()
            .and_then(|cfg| cfg.try_deserialize::<HashMap<String, HashMap<String, Value>>>());
            
        match config_result {
            Ok(config) => {
                debug!("Successfully loaded config file: {}", path);
                Ok(config)
            }
            Err(e) => {
                // Check if it's a file not found error (which is acceptable)
                if e.to_string().contains("not found") || e.to_string().contains("No such file") {
                    warn!(target: "cfg", "Config file not found: {}. Using default configuration.", path.blue());
                    Ok(HashMap::new())
                } else {
                    // Other errors are more serious
                    warn!("Failed to read config file {}: {}. Using default configuration.", path.blue(), e);
                    Ok(HashMap::new())
                }
            }
        }
    }
    
    /// Determine organization from environment variables or use default
    pub fn determine_org(&self, env_vars: &HashMap<String, Value>) -> String {
        env_vars
            .get("org")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                warn!(
                    target: "cfg", 
                    "{}_ORG is not provided. Using: {}", 
                    self.env_prefix,
                    "cubtera".blue()
                );
                "cubtera".to_string()
            })
    }
    
    /// Merge configuration from multiple sources with priority:
    /// 1. Environment variables (highest priority)
    /// 2. Organization-specific config section
    /// 3. Default config section (lowest priority)
    pub fn merge_config_sources(
        &self,
        config_file: HashMap<String, HashMap<String, Value>>,
        env_vars: HashMap<String, Value>,
        org: &str,
    ) -> HashMap<String, Value> {
        debug!("Merging config sources for org: {}", org);
        
        // Start with default section from config file
        let mut merged_config = config_file.get("default").cloned().unwrap_or_default();
        
        // Override with organization-specific section
        if let Some(org_config) = config_file.get(org) {
            debug!("Found organization-specific config for: {}", org);
            merged_config.extend(org_config.clone());
        }
        
        // Override with environment variables (highest priority)
        merged_config.extend(env_vars);
        
        debug!("Merged config has {} keys", merged_config.len());
        merged_config
    }
    
    /// Validate that required configuration values are present
    pub fn validate_config(&self, config: &HashMap<String, Value>) -> ConfigResult<()> {
        // Basic validation - just check for obviously invalid values
        // Detailed validation will be done in CubteraConfig::validate()
        
        if let Some(workspace_path) = config.get("workspace_path") {
            if let Some(path_str) = workspace_path.as_str() {
                if path_str.is_empty() {
                    return Err(ConfigError::validation("workspace_path", "cannot be empty"));
                }
            }
        }
        
        if let Some(org) = config.get("org") {
            if let Some(org_str) = org.as_str() {
                if org_str.is_empty() {
                    return Err(ConfigError::validation("org", "cannot be empty"));
                }
            }
        }
        
        Ok(())
    }
    
    /// Load complete configuration from all sources
    pub fn load_complete_config(&self) -> ConfigResult<HashMap<String, Value>> {
        // Load environment variables
        let env_vars = self.load_env_vars()?;
        
        // Determine config file path
        let config_path = self.determine_config_path(&env_vars)?;
        
        // Load config file
        let config_file = self.load_config_file(&config_path)?;
        
        // Determine organization
        let org = self.determine_org(&env_vars);
        
        // Merge all sources
        let merged_config = self.merge_config_sources(config_file, env_vars, &org);
        
        // Validate configuration
        self.validate_config(&merged_config)?;
        
        if merged_config.is_empty() {
            warn!(target: "cfg", "No configuration found. Using default values");
        } else {
            debug!("Configuration loaded successfully with {} keys", merged_config.len());
        }
        
        Ok(merged_config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::env;
    
    #[test]
    fn test_config_loader_creation() {
        let loader = ConfigLoader::new();
        assert_eq!(loader.env_prefix, "CUBTERA");
        assert_eq!(loader.default_config_path, "~/.cubtera/config.toml");
    }
    
    #[test]
    fn test_config_loader_with_custom_prefix() {
        let loader = ConfigLoader::new().with_env_prefix("TEST");
        assert_eq!(loader.env_prefix, "TEST");
    }
    
    #[test]
    fn test_config_loader_with_custom_path() {
        let loader = ConfigLoader::new().with_default_config_path("/custom/path.toml");
        assert_eq!(loader.default_config_path, "/custom/path.toml");
    }
    
    #[test]
    fn test_determine_org_from_env() {
        let loader = ConfigLoader::new();
        let mut env_vars = HashMap::new();
        env_vars.insert("org".to_string(), json!("myorg"));
        
        let org = loader.determine_org(&env_vars);
        assert_eq!(org, "myorg");
    }
    
    #[test]
    fn test_determine_org_default() {
        let loader = ConfigLoader::new();
        let env_vars = HashMap::new();
        
        let org = loader.determine_org(&env_vars);
        assert_eq!(org, "cubtera");
    }
    
    #[test]
    fn test_merge_config_sources() {
        let loader = ConfigLoader::new();
        
        // Config file with default and org sections
        let mut config_file = HashMap::new();
        let mut default_section = HashMap::new();
        default_section.insert("key1".to_string(), json!("default_value1"));
        default_section.insert("key2".to_string(), json!("default_value2"));
        config_file.insert("default".to_string(), default_section);
        
        let mut org_section = HashMap::new();
        org_section.insert("key2".to_string(), json!("org_value2"));
        org_section.insert("key3".to_string(), json!("org_value3"));
        config_file.insert("myorg".to_string(), org_section);
        
        // Environment variables
        let mut env_vars = HashMap::new();
        env_vars.insert("key3".to_string(), json!("env_value3"));
        env_vars.insert("key4".to_string(), json!("env_value4"));
        
        let merged = loader.merge_config_sources(config_file, env_vars, "myorg");
        
        // Check priority: env > org > default
        assert_eq!(merged.get("key1").unwrap(), &json!("default_value1")); // from default
        assert_eq!(merged.get("key2").unwrap(), &json!("org_value2"));     // from org (overrides default)
        assert_eq!(merged.get("key3").unwrap(), &json!("env_value3"));     // from env (overrides org)
        assert_eq!(merged.get("key4").unwrap(), &json!("env_value4"));     // from env only
    }
    
    #[test]
    fn test_validate_config_empty_workspace_path() {
        let loader = ConfigLoader::new();
        let mut config = HashMap::new();
        config.insert("workspace_path".to_string(), json!(""));
        
        let result = loader.validate_config(&config);
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_config_valid() {
        let loader = ConfigLoader::new();
        let mut config = HashMap::new();
        config.insert("workspace_path".to_string(), json!("/valid/path"));
        
        let result = loader.validate_config(&config);
        assert!(result.is_ok());
    }
} 