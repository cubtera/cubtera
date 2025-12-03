// TODO: Implement comprehensive JSON tests
// - read_json_file tests with valid/invalid JSON
// - merge_values tests with complex nested objects
// - validate_json_by_schema tests
// - error scenarios (file not found, invalid JSON, schema validation failures)
// - performance tests with large JSON files
// - Unicode and special character tests 

#[cfg(test)]
mod tests {
    use crate::tools::json::*;
    use crate::tools::error::ToolsError;
    use serde_json::{json, Value};
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn test_read_json_file_success() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("test.json");
        
        let json_content = json!({
            "name": "test",
            "version": "1.0.0",
            "features": ["feature1", "feature2"],
            "config": {
                "debug": true,
                "timeout": 30
            }
        });
        
        fs::write(&file_path, json_content.to_string()).unwrap();
        
        let result = read_json_file(&file_path);
        assert!(result.is_ok());
        
        let loaded_json = result.unwrap();
        assert_eq!(loaded_json["name"], "test");
        assert_eq!(loaded_json["version"], "1.0.0");
        assert_eq!(loaded_json["config"]["debug"], true);
        assert_eq!(loaded_json["config"]["timeout"], 30);
    }

    #[test]
    fn test_read_json_file_not_exists() {
        let non_existent = PathBuf::from("/non/existent/file.json");
        let result = read_json_file(&non_existent);
        
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::FileNotFound { .. } => (),
            _ => panic!("Expected FileNotFound error"),
        }
    }

    #[test]
    fn test_read_json_file_invalid_json() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("invalid.json");
        
        // Write invalid JSON
        fs::write(&file_path, "{ invalid json content }").unwrap();
        
        let result = read_json_file(&file_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::Validation { .. } => (),
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_read_json_file_empty_file() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("empty.json");
        
        fs::write(&file_path, "").unwrap();
        
        let result = read_json_file(&file_path);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::Validation { .. } => (),
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_read_json_file_unicode_content() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("unicode.json");
        
        let json_content = json!({
            "русский": "текст",
            "中文": "内容",
            "emoji": "🚀🎉",
            "mixed": "Hello мир 世界 🌍"
        });
        
        fs::write(&file_path, json_content.to_string()).unwrap();
        
        let result = read_json_file(&file_path);
        assert!(result.is_ok());
        
        let loaded_json = result.unwrap();
        assert_eq!(loaded_json["русский"], "текст");
        assert_eq!(loaded_json["中文"], "内容");
        assert_eq!(loaded_json["emoji"], "🚀🎉");
    }

    #[test]
    fn test_merge_values_simple() {
        let mut target = json!({"a": 1, "b": 2});
        let source = json!({"c": 3, "d": 4});
        
        merge_values(&mut target, &source);
        
        assert_eq!(target["a"], 1);
        assert_eq!(target["b"], 2);
        assert_eq!(target["c"], 3);
        assert_eq!(target["d"], 4);
    }

    #[test]
    fn test_merge_values_nested_objects() {
        let mut target = json!({
            "config": {
                "database": {
                    "host": "localhost",
                    "port": 5432
                },
                "cache": {
                    "enabled": true
                }
            },
            "version": "1.0.0"
        });
        
        let source = json!({
            "config": {
                "database": {
                    "name": "mydb",
                    "ssl": true
                },
                "logging": {
                    "level": "info"
                }
            },
            "author": "test"
        });
        
        merge_values(&mut target, &source);
        
        // Original values should be preserved
        assert_eq!(target["config"]["database"]["host"], "localhost");
        assert_eq!(target["config"]["database"]["port"], 5432);
        assert_eq!(target["config"]["cache"]["enabled"], true);
        assert_eq!(target["version"], "1.0.0");
        
        // New values should be added
        assert_eq!(target["config"]["database"]["name"], "mydb");
        assert_eq!(target["config"]["database"]["ssl"], true);
        assert_eq!(target["config"]["logging"]["level"], "info");
        assert_eq!(target["author"], "test");
    }

    #[test]
    fn test_merge_values_overwrite_non_objects() {
        let mut target = json!({"key": "original_value"});
        let source = json!({"key": "new_value"});
        
        merge_values(&mut target, &source);
        
        // Non-object values should not be merged, original should be preserved
        assert_eq!(target["key"], "original_value");
    }

    #[test]
    fn test_merge_values_arrays() {
        let mut target = json!({"items": [1, 2, 3]});
        let source = json!({"items": [4, 5, 6]});
        
        merge_values(&mut target, &source);
        
        // Arrays should not be merged, original should be preserved
        assert_eq!(target["items"], json!([1, 2, 3]));
    }

    #[test]
    fn test_merge_values_empty_objects() {
        let mut target = json!({});
        let source = json!({"key": "value"});
        
        merge_values(&mut target, &source);
        
        assert_eq!(target["key"], "value");
    }

    #[test]
    fn test_merge_values_non_objects() {
        let mut target = json!("string");
        let source = json!({"key": "value"});
        
        merge_values(&mut target, &source);
        
        // Should not merge if target is not an object
        assert_eq!(target, json!("string"));
    }

    #[test]
    fn test_validate_json_by_schema_success() {
        let json_data = json!({
            "name": "John Doe",
            "age": 30,
            "email": "john@example.com"
        });
        
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"},
                "email": {"type": "string"}
            },
            "required": ["name", "age"]
        });
        
        let result = validate_json_by_schema(&json_data, &schema);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), json_data);
    }

    #[test]
    fn test_validate_json_by_schema_missing_required() {
        let json_data = json!({
            "name": "John Doe"
            // Missing required "age" field
        });
        
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });
        
        let result = validate_json_by_schema(&json_data, &schema);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::Validation { .. } => (),
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_validate_json_by_schema_wrong_type() {
        let json_data = json!({
            "name": "John Doe",
            "age": "thirty" // Should be number, not string
        });
        
        let schema = json!({
            "type": "object",
            "properties": {
                "name": {"type": "string"},
                "age": {"type": "number"}
            },
            "required": ["name", "age"]
        });
        
        let result = validate_json_by_schema(&json_data, &schema);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::Validation { .. } => (),
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_validate_json_by_schema_invalid_schema() {
        let json_data = json!({"name": "test"});
        let invalid_schema = json!({"type": "invalid_type"});
        
        let result = validate_json_by_schema(&json_data, &invalid_schema);
        assert!(result.is_err());
        match result.unwrap_err() {
            ToolsError::Validation { .. } => (),
            _ => panic!("Expected Validation error"),
        }
    }

    #[test]
    fn test_validate_json_by_schema_array() {
        let json_data = json!([1, 2, 3, 4, 5]);
        
        let schema = json!({
            "type": "array",
            "items": {"type": "number"},
            "minItems": 3,
            "maxItems": 10
        });
        
        let result = validate_json_by_schema(&json_data, &schema);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_json_by_schema_nested_objects() {
        let json_data = json!({
            "user": {
                "profile": {
                    "name": "John",
                    "settings": {
                        "theme": "dark",
                        "notifications": true
                    }
                }
            }
        });
        
        let schema = json!({
            "type": "object",
            "properties": {
                "user": {
                    "type": "object",
                    "properties": {
                        "profile": {
                            "type": "object",
                            "properties": {
                                "name": {"type": "string"},
                                "settings": {
                                    "type": "object",
                                    "properties": {
                                        "theme": {"type": "string"},
                                        "notifications": {"type": "boolean"}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        });
        
        let result = validate_json_by_schema(&json_data, &schema);
        assert!(result.is_ok());
    }

    // Performance tests
    #[test]
    fn test_performance_large_json() {
        use std::time::Instant;
        
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("large.json");
        
        // Create a large JSON object
        let mut large_object = serde_json::Map::new();
        for i in 0..1000 {
            large_object.insert(
                format!("key_{}", i),
                json!({
                    "id": i,
                    "name": format!("item_{}", i),
                    "data": vec![i; 10],
                    "metadata": {
                        "created": "2023-01-01",
                        "updated": "2023-12-31"
                    }
                })
            );
        }
        let large_json = Value::Object(large_object);
        
        fs::write(&file_path, large_json.to_string()).unwrap();
        
        let start = Instant::now();
        let result = read_json_file(&file_path);
        let duration = start.elapsed();
        
        assert!(result.is_ok());
        assert!(duration.as_millis() < 1000, "Reading large JSON took too long: {:?}", duration);
        
        let loaded = result.unwrap();
        assert_eq!(loaded["key_0"]["id"], 0);
        assert_eq!(loaded["key_999"]["id"], 999);
    }

    #[test]
    fn test_performance_merge_large_objects() {
        use std::time::Instant;
        
        // Create large objects for merging
        let mut target = serde_json::Map::new();
        let mut source = serde_json::Map::new();
        
        for i in 0..500 {
            target.insert(format!("target_key_{}", i), json!({"value": i}));
            source.insert(format!("source_key_{}", i), json!({"value": i + 1000}));
        }
        
        let mut target_json = Value::Object(target);
        let source_json = Value::Object(source);
        
        let start = Instant::now();
        merge_values(&mut target_json, &source_json);
        let duration = start.elapsed();
        
        assert!(duration.as_millis() < 100, "Merging large objects took too long: {:?}", duration);
        
        // Verify merge worked
        assert_eq!(target_json["target_key_0"]["value"], 0);
        assert_eq!(target_json["source_key_0"]["value"], 1000);
    }

    // Edge cases
    #[test]
    fn test_read_json_file_very_nested() {
        let temp_dir = TempDir::new().unwrap();
        let file_path = temp_dir.path().join("nested.json");
        
        let deeply_nested = json!({
            "level1": {
                "level2": {
                    "level3": {
                        "level4": {
                            "level5": {
                                "value": "deep"
                            }
                        }
                    }
                }
            }
        });
        
        fs::write(&file_path, deeply_nested.to_string()).unwrap();
        
        let result = read_json_file(&file_path);
        assert!(result.is_ok());
        
        let loaded = result.unwrap();
        assert_eq!(loaded["level1"]["level2"]["level3"]["level4"]["level5"]["value"], "deep");
    }

    #[test]
    fn test_merge_values_circular_like_structure() {
        let mut target = json!({
            "a": {"ref": "b"},
            "b": {"ref": "a"}
        });
        
        let source = json!({
            "a": {"new_field": "value"},
            "c": {"ref": "a"}
        });
        
        merge_values(&mut target, &source);
        
        assert_eq!(target["a"]["ref"], "b");
        assert_eq!(target["a"]["new_field"], "value");
        assert_eq!(target["b"]["ref"], "a");
        assert_eq!(target["c"]["ref"], "a");
    }
}