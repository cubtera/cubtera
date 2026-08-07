use super::mock_helpers::*;
use serde_json::{json, Value};

#[cfg(test)]
mod data_operations_tests {
    use super::*;

    #[test]
    fn test_data_merging_basic() {
        let (default_data, override_data) = create_merge_test_data();
        let mut builder = create_test_dim_builder();
        
        builder.default_data = default_data;
        builder.data = override_data;
        
        let merged = builder.merge_defaults();
        
        // Override values should take precedence
        assert_eq!(merged.data["string_key"], "override_string");
        assert_eq!(merged.data["number_key"], 84);
        
        // Default-only values should be preserved
        assert_eq!(merged.data["default_only_key"], "default_only_value");
        
        // Override-only values should be present
        assert_eq!(merged.data["override_only_key"], "override_only_value");
    }

    #[test]
    fn test_data_merging_nested_objects() {
        let (default_data, override_data) = create_merge_test_data();
        let mut builder = create_test_dim_builder();
        
        builder.default_data = default_data;
        builder.data = override_data;
        
        let merged = builder.merge_defaults();
        
        // Nested object should be merged, not replaced
        let object_key = &merged.data["object_key"];
        assert_eq!(object_key["nested_string"], "override_nested");
        assert_eq!(object_key["nested_number"], 100); // From default
        assert_eq!(object_key["nested_bool"], false); // From override
        assert_eq!(object_key["default_only"], "only_in_default");
        assert_eq!(object_key["override_only"], "only_in_override");
    }

    #[test]
    fn test_data_merging_arrays() {
        let (default_data, override_data) = create_merge_test_data();
        let mut builder = create_test_dim_builder();
        
        builder.default_data = default_data;
        builder.data = override_data;
        
        let merged = builder.merge_defaults();
        
        // Arrays should be replaced, not merged
        let array_key = &merged.data["array_key"];
        assert_eq!(array_key.as_array().unwrap().len(), 3);
        assert_eq!(array_key[0], "override1");
        assert_eq!(array_key[1], "override2");
        assert_eq!(array_key[2], "override3");
    }

    #[test]
    fn test_get_data_methods() {
        let builder = create_test_dim_builder();
        let data = builder.get_data();
        
        assert_eq!(data["name"], "test_dim");
        assert_eq!(data["meta"]["region"], "us-east-1");
    }

    #[test]
    fn test_get_default_data() {
        let builder = create_test_dim_builder();
        let default_data = builder.get_default_data();
        
        assert_eq!(default_data["default_key"], "default_value");
        assert_eq!(default_data["meta"]["default_region"], "us-west-1");
    }

    #[test]
    fn test_builder_with_name_updates_data() {
        let builder = create_test_dim_builder()
            .with_name("new_name");
        
        assert_eq!(builder.dim_name, "new_name");
        assert_eq!(builder.data["name"], "new_name");
    }

    #[test]
    fn test_empty_data_merging() {
        let mut builder = create_test_dim_builder();
        builder.default_data = json!({});
        builder.data = json!({});
        
        let merged = builder.merge_defaults();
        assert!(merged.data.as_object().unwrap().is_empty());
    }

    #[test]
    fn test_null_value_handling() {
        let mut builder = create_test_dim_builder();
        builder.default_data = json!({
            "null_key": Value::Null,
            "string_key": "default"
        });
        builder.data = json!({
            "null_key": "override",
            "other_key": Value::Null
        });
        
        let merged = builder.merge_defaults();
        assert_eq!(merged.data["null_key"], "override");
        assert_eq!(merged.data["string_key"], "default");
        assert_eq!(merged.data["other_key"], Value::Null);
    }

    #[test]
    fn test_complex_nested_merging() {
        let mut builder = create_test_dim_builder();
        builder.default_data = json!({
            "level1": {
                "level2": {
                    "default_value": "default",
                    "shared_value": "from_default"
                },
                "default_level2": "default"
            }
        });
        builder.data = json!({
            "level1": {
                "level2": {
                    "override_value": "override",
                    "shared_value": "from_override"
                },
                "override_level2": "override"
            }
        });
        
        let merged = builder.merge_defaults();
        let level1 = &merged.data["level1"];
        let level2 = &level1["level2"];
        
        assert_eq!(level2["default_value"], "default");
        assert_eq!(level2["override_value"], "override");
        assert_eq!(level2["shared_value"], "from_override");
        assert_eq!(level1["default_level2"], "default");
        assert_eq!(level1["override_level2"], "override");
    }
} 