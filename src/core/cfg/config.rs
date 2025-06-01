use super::error::{ConfigError, ConfigResult, ConfigResultExt};
use super::loader::ConfigLoader;
use crate::tools::{convert_path_to_absolute, db};
use crate::utils::helper::*;
use log::{debug, warn};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Add;

/// Main configuration structure for Cubtera
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CubteraConfig {
    #[serde(default = "default_workspace_path")]
    pub workspace_path: String,
    #[serde(default = "default_inventory_path")]
    pub inventory_path: String,
    #[serde(default = "default_units_path")]
    pub units_path: String,
    #[serde(default = "default_modules_path")]
    pub modules_path: String,
    #[serde(default = "default_plugins_path")]
    pub plugins_path: String,
    #[serde(default = "default_org")]
    pub org: String,
    #[serde(default = "default_temp_folder_path")]
    pub temp_folder_path: String,
    #[serde(
        deserialize_with = "deserialize_colon_list",
        serialize_with = "serialize_colon_list",
        default = "default_orgs"
    )]
    pub orgs: Vec<String>,
    #[serde(
        deserialize_with = "deserialize_colon_list",
        serialize_with = "serialize_colon_list",
        default = "default_dim_relations"
    )]
    pub dim_relations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_db: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_user_name_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_number_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_name_env: Option<String>,
    #[serde(default)]
    pub clean_cache: bool,
    #[serde(default)]
    pub always_copy_files: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<HashMap<String, HashMap<String, String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HashMap<String, HashMap<String, String>>>,
    #[serde(skip)]
    pub db_client: Option<mongodb::sync::Client>,
    #[serde(default = "default_file_name_separator")]
    pub file_name_separator: String,
}

// Default value functions
fn default_workspace_path() -> String {
    "~/.cubtera/workspace".into()
}

fn default_temp_folder_path() -> String {
    "~/.cubtera/tmp".into()
}

fn default_units_folder() -> String {
    "units".into()
}

fn default_units_path() -> String {
    default_workspace_path().add(&format!("/{}", default_units_folder()))
}

fn default_modules_folder() -> String {
    "modules".into()
}

fn default_modules_path() -> String {
    default_workspace_path().add(&format!("/{}", default_modules_folder()))
}

fn default_plugins_folder() -> String {
    "plugins".into()
}

fn default_plugins_path() -> String {
    default_workspace_path().add(&format!("/{}", default_plugins_folder()))
}

fn default_inventory_folder() -> String {
    "inventory".into()
}

fn default_inventory_path() -> String {
    default_workspace_path().add(&format!("/{}", default_inventory_folder()))
}

fn default_dim_relations() -> Vec<String> {
    ["dome".into(), "env".into(), "dc".into()].into()
}

fn default_orgs() -> Vec<String> {
    ["cubtera".into()].into()
}

fn default_org() -> String {
    "cubtera".into()
}

fn default_file_name_separator() -> String {
    ":".into()
}

impl Default for CubteraConfig {
    fn default() -> Self {
        Self {
            workspace_path: default_workspace_path(),
            inventory_path: default_inventory_path(),
            units_path: default_units_path(),
            modules_path: default_modules_path(),
            plugins_path: default_plugins_path(),
            temp_folder_path: default_temp_folder_path(),
            org: default_org(),
            orgs: default_orgs(),
            dim_relations: default_dim_relations(),
            db: None,
            dlog_db: None,
            dlog_job_user_name_env: None,
            dlog_job_number_env: None,
            dlog_job_name_env: None,
            clean_cache: false,
            always_copy_files: true,
            runner: None,
            state: None,
            db_client: None,
            file_name_separator: default_file_name_separator(),
        }
    }
}

impl CubteraConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Load configuration from all sources (env vars, config file, defaults)
    pub fn load() -> ConfigResult<Self> {
        let loader = ConfigLoader::new();
        let config_data = loader.load_complete_config()?;
        
        if config_data.is_empty() {
            debug!("No configuration data found, using defaults");
            return Ok(Self::default());
        }
        
        Self::from_config_data(config_data)
    }
    
    /// Load configuration with custom loader
    pub fn load_with_loader(loader: ConfigLoader) -> ConfigResult<Self> {
        let config_data = loader.load_complete_config()?;
        
        if config_data.is_empty() {
            debug!("No configuration data found, using defaults");
            return Ok(Self::default());
        }
        
        Self::from_config_data(config_data)
    }
    
    /// Create configuration from raw config data
    fn from_config_data(config_data: HashMap<String, Value>) -> ConfigResult<Self> {
        debug!("Creating Cubtera config from {} config values", config_data.len());
        
        // Convert HashMap to JSON Value for deserialization
        let config_value = serde_json::to_value(&config_data)
            .serialization_error("json")?;
        
        // Deserialize to CubteraConfig
        let mut config = serde_json::from_value::<CubteraConfig>(config_value)
            .serialization_error("json")?;
        
        // Post-process paths
        config.resolve_paths(&config_data)?;
        
        // Initialize database connection if configured
        config.initialize_database()?;
        
        debug!("Cubtera config created successfully");
        Ok(config)
    }
    
    /// Resolve and validate all paths in the configuration
    fn resolve_paths(&mut self, config_data: &HashMap<String, Value>) -> ConfigResult<()> {
        debug!("Resolving configuration paths");
        
        // Resolve workspace path
        self.workspace_path = convert_path_to_absolute(self.workspace_path.clone())
            .unwrap_or_else(|_| {
                warn!("Failed to resolve workspace_path: {}", self.workspace_path);
                self.workspace_path.clone()
            });
        
        // Resolve other paths based on workspace or explicit configuration
        self.modules_path = self.resolve_path(config_data, "modules")?;
        self.units_path = self.resolve_path(config_data, "units")?;
        self.plugins_path = self.resolve_path(config_data, "plugins")?;
        self.inventory_path = self.resolve_path(config_data, "inventory")?;
        
        // Resolve temp folder path
        self.temp_folder_path = convert_path_to_absolute(self.temp_folder_path.clone())
            .unwrap_or_else(|_| {
                warn!("Failed to resolve temp_folder_path: {}", self.temp_folder_path);
                self.temp_folder_path.clone()
            });
        
        debug!("All paths resolved successfully");
        Ok(())
    }
    
    /// Resolve a specific path type (modules, units, plugins, inventory)
    fn resolve_path(&self, config_data: &HashMap<String, Value>, path_type: &str) -> ConfigResult<String> {
        let path = config_data
            .get(&format!("{}_path", path_type))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                // Use folder name if specified, otherwise use default
                let folder_name = config_data
                    .get(&format!("{}_folder", path_type))
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| match path_type {
                        "plugins" => default_plugins_folder(),
                        "modules" => default_modules_folder(),
                        "units" => default_units_folder(),
                        "inventory" => default_inventory_folder(),
                        _ => path_type.to_string(),
                    });
                
                format!("{}/{}", self.workspace_path, folder_name)
            });
        
        // Convert to absolute path
        let absolute_path = convert_path_to_absolute(path.clone())
            .unwrap_or_else(|_| {
                warn!("Failed to resolve {}_path: {}", path_type, path);
                path.clone()
            });
        
        Ok(absolute_path)
    }
    
    /// Initialize database connection if configured
    fn initialize_database(&mut self) -> ConfigResult<()> {
        if let Some(db_connection_string) = &self.db {
            debug!("Initializing database connection");
            
            match db::connect(db_connection_string) {
                Ok(client) => {
                    self.db_client = Some(client);
                    debug!("Database connection established successfully");
                }
                Err(e) => {
                    warn!("Failed to connect to database: {}", e);
                    // Don't fail the entire config loading for database connection issues
                    // Just log the warning and continue without database
                }
            }
        }
        
        Ok(())
    }
    
    /// Get configuration as JSON Value
    pub fn get_values(&self) -> ConfigResult<serde_json::Value> {
        serde_json::to_value(self)
            .serialization_error("json")
    }
    
    /// Get database client if available
    pub fn get_db(&self) -> Option<mongodb::sync::Client> {
        self.db_client.clone()
    }
    
    /// Get runner configuration by type
    pub fn get_runner_by_type(&self, runner_type: &str) -> Option<HashMap<String, String>> {
        self.runner
            .as_ref()
            .and_then(|r| r.get(runner_type).cloned())
    }
    
    /// Get configuration as pretty-printed JSON
    pub fn get_json(&self) -> ConfigResult<String> {
        serde_json::to_string_pretty(self)
            .serialization_error("json")
    }
    
    /// Get configuration as TOML
    pub fn get_toml(&self) -> ConfigResult<String> {
        toml::to_string_pretty(self)
            .serialization_error("toml")
    }
    
    /// Validate the configuration
    pub fn validate(&self) -> ConfigResult<()> {
        // Validate workspace path
        if self.workspace_path.is_empty() {
            return Err(ConfigError::validation("workspace_path", "cannot be empty"));
        }
        
        // Validate organization
        if self.org.is_empty() {
            return Err(ConfigError::validation("org", "cannot be empty"));
        }
        
        // Validate that orgs list contains the current org
        if !self.orgs.contains(&self.org) {
            warn!("Current org '{}' is not in the orgs list: {:?}", self.org, self.orgs);
        }
        
        // Validate file name separator
        if self.file_name_separator.is_empty() {
            return Err(ConfigError::validation("file_name_separator", "cannot be empty"));
        }
        
        debug!("Configuration validation passed");
        Ok(())
    }
}

// Serde helper functions for colon-separated lists
fn deserialize_colon_list<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    Ok(s.split(':').map(|s| s.to_string()).collect())
}

#[allow(clippy::ptr_arg)]
fn serialize_colon_list<S>(list: &Vec<String>, serializer: S) -> std::result::Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let joined = list.join(":");
    serializer.serialize_str(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::env;
    
    #[test]
    fn test_default_config() {
        let config = CubteraConfig::default();
        assert_eq!(config.org, "cubtera");
        assert_eq!(config.workspace_path, "~/.cubtera/workspace");
        assert!(!config.clean_cache);
        assert!(config.always_copy_files);
    }
    
    #[test]
    fn test_new_config() {
        let config = CubteraConfig::new();
        assert_eq!(config.org, "cubtera");
    }
    
    #[test]
    fn test_from_config_data() {
        let mut config_data = HashMap::new();
        config_data.insert("org".to_string(), json!("testorg"));
        config_data.insert("workspace_path".to_string(), json!("/test/workspace"));
        config_data.insert("clean_cache".to_string(), json!(true));
        
        let config = CubteraConfig::from_config_data(config_data).unwrap();
        assert_eq!(config.org, "testorg");
        assert!(config.clean_cache);
    }
    
    #[test]
    fn test_validate_valid_config() {
        let config = CubteraConfig::default();
        assert!(config.validate().is_ok());
    }
    
    #[test]
    fn test_validate_empty_workspace_path() {
        let mut config = CubteraConfig::default();
        config.workspace_path = String::new();
        
        let result = config.validate();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_validate_empty_org() {
        let mut config = CubteraConfig::default();
        config.org = String::new();
        
        let result = config.validate();
        assert!(result.is_err());
    }
    
    #[test]
    fn test_get_runner_by_type() {
        let mut config = CubteraConfig::default();
        let mut runners = HashMap::new();
        let mut terraform_config = HashMap::new();
        terraform_config.insert("version".to_string(), "1.0.0".to_string());
        runners.insert("terraform".to_string(), terraform_config.clone());
        config.runner = Some(runners);
        
        let result = config.get_runner_by_type("terraform");
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("version").unwrap(), "1.0.0");
        
        let result = config.get_runner_by_type("nonexistent");
        assert!(result.is_none());
    }
    
    #[test]
    fn test_colon_list_serialization() {
        let list = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let serialized = serde_json::to_string(&list).unwrap();
        // This tests the normal JSON serialization, not our custom colon serialization
        assert!(serialized.contains("a") && serialized.contains("b") && serialized.contains("c"));
    }
} 