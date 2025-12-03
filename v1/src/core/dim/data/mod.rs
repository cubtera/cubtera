mod jsonfile;
mod mongodb;

use crate::globals::GLOBAL_CFG;
use serde_json::Value;

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
}

pub fn data_src_init(org: &str, dim_type: &str, storage: Storage) -> Box<dyn DataSource> {
    match storage {
        Storage::DB => Box::new(mongodb::MongoDBDataSource::new(org, dim_type)),
        Storage::FS => Box::new(jsonfile::JsonDataSource::new(
            org,
            dim_type,
            &GLOBAL_CFG.inventory_path,
        )),
        //_ => unreachable!("Unknown storage type")
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
    use serde_json::json;

    // ========== Storage Enum Tests ==========

    #[test]
    fn test_storage_default() {
        let storage = Storage::default();
        assert_eq!(storage, Storage::FS);
    }

    #[test]
    fn test_storage_from_str_fs() {
        let storage = Storage::from_str("fs");
        assert_eq!(storage, Storage::FS);
    }

    #[test]
    fn test_storage_from_str_db() {
        let storage = Storage::from_str("db");
        assert_eq!(storage, Storage::DB);
    }

    #[test]
    #[should_panic(expected = "Unknown storage type")]
    fn test_storage_from_str_unknown() {
        Storage::from_str("unknown");
    }

    #[test]
    fn test_storage_to_str_fs() {
        let storage = Storage::FS;
        assert_eq!(storage.to_str(), "fs");
    }

    #[test]
    fn test_storage_to_str_db() {
        let storage = Storage::DB;
        assert_eq!(storage.to_str(), "db");
    }

    #[test]
    fn test_storage_get_defaults_prefix_fs() {
        let storage = Storage::FS;
        assert_eq!(storage.get_defaults_prefix(), ".defaults:");
    }

    #[test]
    fn test_storage_get_defaults_prefix_db() {
        let storage = Storage::DB;
        assert_eq!(storage.get_defaults_prefix(), "_defaults:");
    }

    #[test]
    fn test_storage_clone() {
        let storage = Storage::DB;
        let cloned = storage.clone();
        assert_eq!(storage, cloned);
    }

    #[test]
    fn test_storage_debug() {
        let storage = Storage::FS;
        let debug_str = format!("{:?}", storage);
        assert_eq!(debug_str, "FS");
    }

    #[test]
    fn test_storage_equality() {
        assert_eq!(Storage::FS, Storage::FS);
        assert_eq!(Storage::DB, Storage::DB);
        assert_ne!(Storage::FS, Storage::DB);
    }

    // ========== Mock DataSource for Testing Default Implementations ==========

    #[derive(Clone)]
    struct MockDataSource {
        context: Option<String>,
    }

    impl MockDataSource {
        fn new() -> Self {
            Self { context: None }
        }
    }

    impl DataSource for MockDataSource {
        fn get_data_by_name(&self, _name: &str) -> Result<Value, Box<dyn std::error::Error>> {
            Ok(json!({"test": "data"}))
        }

        fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
            Ok(vec![json!({"name": "test1"}), json!({"name": "test2"})])
        }

        fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
            Ok(vec!["test1".to_string(), "test2".to_string()])
        }

        fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
            Ok(vec!["env".to_string(), "dc".to_string()])
        }

        fn set_context(&mut self, context: Option<String>) {
            self.context = context;
        }

        fn get_context(&self) -> Option<String> {
            self.context.clone()
        }
    }

    // ========== DataSource Trait Default Implementation Tests ==========

    #[test]
    fn test_datasource_upsert_all_data_default() {
        let ds = MockDataSource::new();
        let data = vec![json!({"name": "test"})];
        let result = ds.upsert_all_data(data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_datasource_upsert_data_by_name_default() {
        let ds = MockDataSource::new();
        let result = ds.upsert_data_by_name("test", json!({"data": "value"}));
        assert!(result.is_ok());
    }

    #[test]
    fn test_datasource_delete_data_by_name_default() {
        let ds = MockDataSource::new();
        let result = ds.delete_data_by_name("test");
        assert!(result.is_ok());
    }

    #[test]
    fn test_datasource_delete_all_by_context_default() {
        let ds = MockDataSource::new();
        let result = ds.delete_all_by_context("test_context");
        assert!(result.is_ok());
    }

    #[test]
    fn test_datasource_context_management() {
        let mut ds = MockDataSource::new();

        // Initially no context
        assert!(ds.get_context().is_none());

        // Set context
        ds.set_context(Some("branch:feature-1".to_string()));
        assert_eq!(ds.get_context(), Some("branch:feature-1".to_string()));

        // Clear context
        ds.set_context(None);
        assert!(ds.get_context().is_none());
    }

    #[test]
    fn test_datasource_get_data_by_name() {
        let ds = MockDataSource::new();
        let result = ds.get_data_by_name("test");

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data["test"], "data");
    }

    #[test]
    fn test_datasource_get_all_data() {
        let ds = MockDataSource::new();
        let result = ds.get_all_data();

        assert!(result.is_ok());
        let data = result.unwrap();
        assert_eq!(data.len(), 2);
    }

    #[test]
    fn test_datasource_get_all_names() {
        let ds = MockDataSource::new();
        let result = ds.get_all_names();

        assert!(result.is_ok());
        let names = result.unwrap();
        assert!(names.contains(&"test1".to_string()));
        assert!(names.contains(&"test2".to_string()));
    }

    #[test]
    fn test_datasource_get_all_types() {
        let ds = MockDataSource::new();
        let result = ds.get_all_types();

        assert!(result.is_ok());
        let types = result.unwrap();
        assert!(types.contains(&"env".to_string()));
        assert!(types.contains(&"dc".to_string()));
    }

    // ========== CloneBox Trait Tests ==========

    #[test]
    fn test_clone_box() {
        let ds = MockDataSource::new();
        let boxed: Box<dyn DataSource> = Box::new(ds);
        let cloned = boxed.clone_box();

        // Verify the cloned datasource works
        let result = cloned.get_all_names();
        assert!(result.is_ok());
    }

    // ========== BSON/JSON Conversion Simulation Tests ==========
    // These test the patterns used in MongoDB operations without actual MongoDB

    #[test]
    fn test_json_to_bson_conversion() {
        use ::mongodb::bson;

        let json_value = json!({
            "name": "test-dim",
            "type": "env",
            "data": {
                "key1": "value1",
                "key2": 42
            }
        });

        // Simulate BSON conversion
        let bson_val = bson::to_bson(&json_value);
        assert!(bson_val.is_ok());

        // Convert back
        let back: Result<Value, _> = bson::from_bson(bson_val.unwrap());
        assert!(back.is_ok());

        let restored = back.unwrap();
        assert_eq!(restored["name"], "test-dim");
        assert_eq!(restored["type"], "env");
        assert_eq!(restored["data"]["key1"], "value1");
        assert_eq!(restored["data"]["key2"], 42);
    }

    #[test]
    fn test_json_to_bson_document() {
        use ::mongodb::bson;

        let json_value = json!({
            "name": "test",
            "context": "branch:feature"
        });

        let doc = bson::to_document(&json_value);
        assert!(doc.is_ok());

        let doc = doc.unwrap();
        assert_eq!(doc.get_str("name"), Ok("test"));
        assert_eq!(doc.get_str("context"), Ok("branch:feature"));
    }

    #[test]
    fn test_bson_doc_filter_pattern() {
        use ::mongodb::bson::doc;

        // Test patterns used in MongoDB queries
        let filter_by_name = doc! { "name": "test-name" };
        assert!(filter_by_name.contains_key("name"));

        let filter_with_context = doc! { "name": "test", "context": "branch:feature" };
        assert!(filter_with_context.contains_key("context"));

        let filter_no_context = doc! { "name": "test", "context": { "$exists": false }};
        assert!(filter_no_context.contains_key("name"));
        assert!(filter_no_context.contains_key("context"));
    }

    #[test]
    fn test_data_manipulation_pattern() {
        // Test pattern for manipulating data before BSON conversion
        let mut data = json!({
            "name": "test",
            "value": 42
        });

        // Add context like MongoDB operations do
        data["context"] = json!("branch:feature");

        assert_eq!(data["name"], "test");
        assert_eq!(data["value"], 42);
        assert_eq!(data["context"], "branch:feature");

        // Remove _id field pattern
        data.as_object_mut().unwrap().remove("_id");
        assert!(data.get("_id").is_none());
    }

    #[test]
    fn test_defaults_name_pattern() {
        let name = "_defaults:global";

        // Test the pattern for checking defaults
        assert!(name.starts_with("_defaults:"));

        let name = ".defaults:global";
        assert!(name.starts_with(".defaults:"));

        let name = "regular-name";
        assert!(!name.starts_with("_defaults:"));
        assert!(!name.starts_with(".defaults:"));
    }

    #[test]
    fn test_context_filter_patterns() {
        // Test various context filter patterns
        let context = Some("branch:feature".to_string());

        // Pattern 1: Check if context exists and is not empty
        if let Some(ctx) = &context {
            if !ctx.is_empty() {
                assert_eq!(ctx, "branch:feature");
            }
        }

        // Pattern 2: No context
        let no_context: Option<String> = None;
        assert!(no_context.is_none());

        // Pattern 3: Empty context
        let empty_context = Some("".to_string());
        if let Some(ctx) = &empty_context {
            assert!(ctx.is_empty());
        }
    }
}
