mod jsonfile;
mod mongodb;

// Re-export concrete implementations for testing
pub use jsonfile::JsonDataSource;
pub use mongodb::MongoDBDataSource;

// New error handling module
/// Error types specific to data source operations
pub mod error {
    use std::fmt;

    /// Represents different types of errors that can occur in data source operations
    #[derive(Debug, Clone)]
    pub enum DataSourceError {
        /// Path or directory not found
        PathNotFound { path: String },
        /// Specific file not found
        FileNotFound { filename: String },
        /// JSON parsing or data format errors
        ParseError { message: String },
        /// Database connection or query errors  
        DatabaseError { message: String },
        /// Configuration-related errors
        ConfigurationError { message: String },
        /// File system I/O errors
        IOError { message: String },
        /// Data validation errors
        ValidationError { message: String },
    }

    impl fmt::Display for DataSourceError {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            match self {
                DataSourceError::PathNotFound { path } => write!(f, "Path not found: {}", path),
                DataSourceError::FileNotFound { filename } => write!(f, "File not found: {}", filename),
                DataSourceError::ParseError { message } => write!(f, "Parse error: {}", message),
                DataSourceError::DatabaseError { message } => write!(f, "Database error: {}", message),
                DataSourceError::ConfigurationError { message } => write!(f, "Configuration error: {}", message),
                DataSourceError::IOError { message } => write!(f, "IO error: {}", message),
                DataSourceError::ValidationError { message } => write!(f, "Validation error: {}", message),
            }
        }
    }

    impl std::error::Error for DataSourceError {}

    /// Convenience Result type for data source operations
    pub type DataResult<T> = Result<T, DataSourceError>;
}

// Re-export for convenience
pub use error::{DataSourceError, DataResult};

use crate::globals::GLOBAL_CFG;
use serde_json::Value;

/// Configuration abstraction for data sources
/// This provides a unified way to access configuration, defaulting to GLOBAL_CFG
/// but allowing explicit overrides when needed
#[derive(Debug, Clone)]
pub struct DataSourceConfig {
    pub inventory_path: String,
    pub file_name_separator: String,
    pub db_client: Option<::mongodb::sync::Client>,
}

impl DataSourceConfig {
    /// Create configuration from GLOBAL_CFG (default behavior)
    pub fn from_global() -> Self {
        Self {
            inventory_path: GLOBAL_CFG.inventory_path.clone(),
            file_name_separator: GLOBAL_CFG.file_name_separator.clone(),
            db_client: GLOBAL_CFG.db_client.clone(),
        }
    }
    
    /// Create custom configuration (for testing or special cases)
    pub fn new(
        inventory_path: String,
        file_name_separator: String,
        db_client: Option<::mongodb::sync::Client>,
    ) -> Self {
        Self {
            inventory_path,
            file_name_separator,
            db_client,
        }
    }
    
    /// Override inventory path while keeping other values from GLOBAL_CFG
    pub fn with_inventory_path(inventory_path: String) -> Self {
        let mut config = Self::from_global();
        config.inventory_path = inventory_path;
        config
    }
    
    /// Override file name separator while keeping other values from GLOBAL_CFG
    pub fn with_file_name_separator(separator: String) -> Self {
        let mut config = Self::from_global();
        config.file_name_separator = separator;
        config
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Storage {
    FS,
    DB,
}

impl Default for Storage {
    fn default() -> Self {
        Storage::FS
    }
}

impl Storage {
    pub fn from_str(s: &str) -> Self {
        match s {
            "fs" => Storage::FS,
            "db" => Storage::DB,
            _ => unreachable!("Unknown storage type"),
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            Storage::FS => "fs",
            Storage::DB => "db",
        }
    }

    // TODO: make configurable
    pub fn get_defaults_prefix(&self) -> &str {
        match self {
            Storage::FS => ".defaults:",
            Storage::DB => "_defaults:",
        }
    }
}

pub trait CloneBox {
    fn clone_box(&self) -> Box<dyn DataSource>;
}

impl<T: DataSource + Clone> CloneBox for T {
    fn clone_box(&self) -> Box<dyn DataSource> {
        Box::new(self.clone())
    }
}

pub trait DataSource: CloneBox + 'static {
    // Legacy methods - kept for backward compatibility
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>>;
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>>;
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;

    // only for DB (json files are source of truth)
    fn upsert_all_data(&self, _data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!("this data source doesn't support upsert_all_data");
        Ok(())
    }

    fn upsert_data_by_name(
        &self,
        name: &str,
        data: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support upsert_data_by_name: {}: {}",
            name,
            serde_json::json!(data)
        );
        Ok(())
    }

    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support delete_data_by_name: {}",
            name
        );
        Ok(())
    }

    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support delete_all_by_context: {}",
            context
        );
        Ok(())
    }

    fn set_context(&mut self, context: Option<String>);
    fn get_context(&self) -> Option<String>;

    // New "safe" methods with proper error types - default implementations call legacy methods
    fn get_data_by_name_safe(&self, name: &str) -> DataResult<Value> {
        self.get_data_by_name(name)
            .map_err(|e| DataSourceError::ParseError { message: e.to_string() })
    }

    fn get_all_data_safe(&self) -> DataResult<Vec<Value>> {
        self.get_all_data()
            .map_err(|e| DataSourceError::ParseError { message: e.to_string() })
    }

    fn get_all_names_safe(&self) -> DataResult<Vec<String>> {
        self.get_all_names()
            .map_err(|e| DataSourceError::ParseError { message: e.to_string() })
    }

    fn get_all_types_safe(&self) -> DataResult<Vec<String>> {
        self.get_all_types()
            .map_err(|e| DataSourceError::ParseError { message: e.to_string() })
    }

    fn upsert_all_data_safe(&self, data: Vec<Value>) -> DataResult<()> {
        self.upsert_all_data(data)
            .map_err(|e| DataSourceError::DatabaseError { message: e.to_string() })
    }

    fn upsert_data_by_name_safe(&self, name: &str, data: Value) -> DataResult<()> {
        self.upsert_data_by_name(name, data)
            .map_err(|e| DataSourceError::DatabaseError { message: e.to_string() })
    }

    fn delete_data_by_name_safe(&self, name: &str) -> DataResult<()> {
        self.delete_data_by_name(name)
            .map_err(|e| DataSourceError::DatabaseError { message: e.to_string() })
    }

    fn delete_all_by_context_safe(&self, context: &str) -> DataResult<()> {
        self.delete_all_by_context(context)
            .map_err(|e| DataSourceError::DatabaseError { message: e.to_string() })
    }
}

/// Create a DataSource using GLOBAL_CFG (legacy function, maintained for compatibility)
pub fn data_src_init(org: &str, dim_type: &str, storage: Storage) -> Box<dyn DataSource> {
    data_src_init_with_config(org, dim_type, storage, &DataSourceConfig::from_global())
}

/// Create a DataSource with explicit configuration
pub fn data_src_init_with_config(
    org: &str, 
    dim_type: &str, 
    storage: Storage, 
    config: &DataSourceConfig
) -> Box<dyn DataSource> {
    match storage {
        Storage::DB => Box::new(mongodb::MongoDBDataSource::new_with_config(org, dim_type, config)),
        Storage::FS => Box::new(jsonfile::JsonDataSource::new_with_config(org, dim_type, config)),
    }
}

// pub enum DataSrc {
//     MongoDB(mongodb::MongoDBDataSource),
//     Json(jsonfile::JsonDataSource),
//     // Add other data source types here...
// }

// impl Clone for DataSrc {
//     fn clone(&self) -> Self {
//         match self {
//             DataSrc::MongoDB(ds) => DataSrc::MongoDB(ds.clone()),
//             DataSrc::Json(ds) => DataSrc::Json(ds.clone()),
//             // Handle other data source types here...
//         }
//     }
// }

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn test_data_source_config_from_global() {
        // Test that we can create config from GLOBAL_CFG
        let config = DataSourceConfig::from_global();
        
        // Should match GLOBAL_CFG values
        assert_eq!(config.inventory_path, GLOBAL_CFG.inventory_path);
        assert_eq!(config.file_name_separator, GLOBAL_CFG.file_name_separator);
    }

    #[test]
    fn test_data_source_config_overrides() {
        let custom_path = "/custom/path".to_string();
        let custom_separator = "___".to_string();
        
        // Test inventory path override
        let config1 = DataSourceConfig::with_inventory_path(custom_path.clone());
        assert_eq!(config1.inventory_path, custom_path);
        assert_eq!(config1.file_name_separator, GLOBAL_CFG.file_name_separator); // Should keep global value
        
        // Test separator override
        let config2 = DataSourceConfig::with_file_name_separator(custom_separator.clone());
        assert_eq!(config2.file_name_separator, custom_separator);
        assert_eq!(config2.inventory_path, GLOBAL_CFG.inventory_path); // Should keep global value
    }

    #[test]
    fn test_data_src_init_backward_compatibility() {
        // Test that legacy function still works
        let datasource = data_src_init("test_org", "test_type", Storage::FS);
        
        // Should be able to use it normally
        assert!(datasource.get_context().is_none());
    }

    #[test]
    fn test_data_src_init_with_custom_config() {
        let dir = tempdir().unwrap();
        let custom_config = DataSourceConfig::with_inventory_path(
            dir.path().to_str().unwrap().to_string()
        );
        
        // Create test directory structure
        let test_path = dir.path().join("test_org").join("test_type");
        fs::create_dir_all(&test_path).unwrap();
        
        // Test new function with custom config
        let datasource = data_src_init_with_config(
            "test_org", 
            "test_type", 
            Storage::FS, 
            &custom_config
        );
        
        // Should work with custom config
        assert!(datasource.get_context().is_none());
        
        // Test that safe methods work
        let names = datasource.get_all_names_safe().unwrap();
        assert!(names.is_empty()); // Empty directory should return empty list
    }
}
