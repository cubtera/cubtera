//! Configuration types and loading

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadFile(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("Missing required configuration: {0}")]
    Missing(String),
}

/// Storage backend type
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StorageBackend {
    /// File system storage
    Fs {
        /// Path to inventory directory
        path: PathBuf,
    },
    /// MongoDB storage
    MongoDB {
        /// Connection string
        connection_string: String,
    },
    /// PostgreSQL storage (future)
    Postgres {
        /// Connection string
        connection_string: String,
    },
}

impl Default for StorageBackend {
    fn default() -> Self {
        Self::Fs {
            path: PathBuf::from("inventory"),
        }
    }
}

/// Main configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
    /// Organization name
    pub org: String,

    /// Storage backend
    #[serde(default)]
    pub storage: StorageBackend,

    /// Path to units directory
    #[serde(default = "default_units_path")]
    pub units_path: PathBuf,

    /// Path to modules directory
    #[serde(default = "default_modules_path")]
    pub modules_path: PathBuf,

    /// Path to plugins directory
    #[serde(default = "default_plugins_path")]
    pub plugins_path: PathBuf,

    /// Dimension relations (hierarchy)
    #[serde(default = "default_dim_relations")]
    pub dim_relations: Vec<String>,

    /// Runner configuration
    #[serde(default)]
    pub runner: RunnerConfig,

    /// State backend configuration
    #[serde(default)]
    pub state: HashMap<String, StateBackendConfig>,

    /// Deployment log configuration
    #[serde(default)]
    pub deployment_log: Option<DeploymentLogConfig>,

    /// Log level
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_units_path() -> PathBuf {
    PathBuf::from("units")
}

fn default_modules_path() -> PathBuf {
    PathBuf::from("modules")
}

fn default_plugins_path() -> PathBuf {
    PathBuf::from("plugins")
}

fn default_dim_relations() -> Vec<String> {
    vec!["dome".to_string(), "env".to_string(), "dc".to_string()]
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            org: "default".to_string(),
            storage: StorageBackend::default(),
            units_path: default_units_path(),
            modules_path: default_modules_path(),
            plugins_path: default_plugins_path(),
            dim_relations: default_dim_relations(),
            runner: RunnerConfig::default(),
            state: HashMap::new(),
            deployment_log: None,
            log_level: default_log_level(),
        }
    }
}

impl Config {
    /// Load configuration from file and environment
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = env::var("CUBTERA_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .map(|h| h.join(".cubtera").join("config.toml"))
                    .unwrap_or_else(|| PathBuf::from("config.toml"))
            });

        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific path
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        let mut config = if path.exists() {
            let content = fs::read_to_string(path)?;
            toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?
        } else {
            Config::default()
        };

        // Override from environment variables
        config.apply_env_overrides();

        Ok(config)
    }

    /// Apply environment variable overrides
    fn apply_env_overrides(&mut self) {
        if let Ok(org) = env::var("CUBTERA_ORG") {
            self.org = org;
        }

        if let Ok(log_level) = env::var("CUBTERA_LOG") {
            self.log_level = log_level;
        }

        if let Ok(db_url) = env::var("CUBTERA_DB") {
            self.storage = StorageBackend::MongoDB {
                connection_string: db_url,
            };
        }

        if let Ok(inventory_path) = env::var("CUBTERA_INVENTORY_PATH") {
            self.storage = StorageBackend::Fs {
                path: PathBuf::from(inventory_path),
            };
        }

        if let Ok(units_path) = env::var("CUBTERA_UNITS_PATH") {
            self.units_path = PathBuf::from(units_path);
        }

        if let Ok(modules_path) = env::var("CUBTERA_MODULES_PATH") {
            self.modules_path = PathBuf::from(modules_path);
        }
    }

    /// Get the inventory path (for FS storage)
    pub fn inventory_path(&self) -> Option<&Path> {
        match &self.storage {
            StorageBackend::Fs { path } => Some(path),
            _ => None,
        }
    }
}

/// Runner configuration
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RunnerConfig {
    /// Default Terraform version
    pub default_tf_version: Option<String>,
    /// Default OpenTofu version
    pub default_tofu_version: Option<String>,
    /// Lock port for preventing concurrent runs
    pub lock_port: Option<u16>,
}

/// State backend configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StateBackendConfig {
    /// Backend-specific options
    #[serde(flatten)]
    pub options: HashMap<String, String>,
}

/// Deployment log configuration
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DeploymentLogConfig {
    /// MongoDB connection string for deployment logs
    pub connection_string: String,
    /// Database name
    #[serde(default = "default_dlog_database")]
    pub database: String,
    /// Collection name
    #[serde(default = "default_dlog_collection")]
    pub collection: String,
}

fn default_dlog_database() -> String {
    "cubtera".to_string()
}

fn default_dlog_collection() -> String {
    "deployments".to_string()
}

// Helper for home directory
mod dirs {
    use std::path::PathBuf;

    pub fn home_dir() -> Option<PathBuf> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
            .map(PathBuf::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.org, "default");
        assert!(matches!(config.storage, StorageBackend::Fs { .. }));
    }

    #[test]
    fn test_storage_backend_fs() {
        let backend = StorageBackend::Fs {
            path: PathBuf::from("/tmp/inventory"),
        };
        assert!(matches!(backend, StorageBackend::Fs { .. }));
    }

    #[test]
    fn test_config_inventory_path() {
        let config = Config {
            storage: StorageBackend::Fs {
                path: PathBuf::from("/tmp/inv"),
            },
            ..Default::default()
        };
        assert_eq!(config.inventory_path(), Some(Path::new("/tmp/inv")));
    }
}

