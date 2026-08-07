use super::super::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use tempfile::TempDir;

/// Create a test Dim with basic data
pub fn create_test_dim() -> Dim {
    Dim {
        dim_name: "test_dim".to_string(),
        dim_type: "test_type".to_string(),
        key_path: PathBuf::from("test_type:test_dim"),
        dim_path: PathBuf::from("/test/inventory/test_org/test_type"),
        parent: None,
        data: json!({
            "name": "test_dim",
            "region": "us-east-1",
            "vpc_cidr": "10.0.0.0/16",
            "meta": {
                "region": "us-east-1",
                "vpc_cidr": "10.0.0.0/16"
            }
        }),
        data_sha: "test_sha".to_string(),
        kids: None,
    }
}

/// Create a test Dim with parent
pub fn create_test_dim_with_parent() -> Dim {
    let parent = create_test_dim();
    let mut parent_dim = parent.clone();
    parent_dim.dim_name = "parent_dim".to_string();
    parent_dim.dim_type = "parent_type".to_string();
    parent_dim.key_path = PathBuf::from("parent_type:parent_dim");
    parent_dim.data = json!({
        "name": "parent_dim",
        "region": "us-west-1",
        "environment": "production",
        "meta": {
            "region": "us-west-1",
            "environment": "production"
        }
    });
    
    Dim {
        dim_name: "child_dim".to_string(),
        dim_type: "child_type".to_string(),
        key_path: PathBuf::from("parent_type:parent_dim/child_type:child_dim"),
        dim_path: PathBuf::from("/test/inventory/test_org/child_type"),
        parent: Some(Box::new(parent_dim)),
        data: json!({
            "name": "child_dim",
            "environment": "test",
            "service": "api",
            "meta": {
                "parent": "parent_type:parent_dim",
                "environment": "test",
                "service": "api"
            }
        }),
        data_sha: "child_sha".to_string(),
        kids: None,
    }
}

/// Create a test Dim with children
pub fn create_test_dim_with_children() -> Dim {
    Dim {
        dim_name: "parent_dim".to_string(),
        dim_type: "parent_type".to_string(),
        key_path: PathBuf::from("parent_type:parent_dim"),
        dim_path: PathBuf::from("/test/inventory/test_org/parent_type"),
        parent: None,
        data: json!({
            "name": "parent_dim",
            "region": "us-east-1",
            "meta": {
                "region": "us-east-1"
            }
        }),
        data_sha: "parent_sha".to_string(),
        kids: Some(vec![
            "child_type:child1".to_string(),
            "child_type:child2".to_string(),
        ]),
    }
}

/// Create a complex hierarchy: grandparent -> parent -> child
pub fn create_test_hierarchy() -> Dim {
    let grandparent = Dim {
        dim_name: "grandparent".to_string(),
        dim_type: "org".to_string(),
        key_path: PathBuf::from("org:grandparent"),
        dim_path: PathBuf::from("/test/inventory/test_org/org"),
        parent: None,
        data: json!({
            "name": "grandparent",
            "level": "org",
            "global_setting": "value1",
            "meta": {
                "level": "org",
                "global_setting": "value1"
            }
        }),
        data_sha: "grandparent_sha".to_string(),
        kids: Some(vec!["dc:parent".to_string()]),
    };

    let parent = Dim {
        dim_name: "parent".to_string(),
        dim_type: "dc".to_string(),
        key_path: PathBuf::from("org:grandparent/dc:parent"),
        dim_path: PathBuf::from("/test/inventory/test_org/dc"),
        parent: Some(Box::new(grandparent)),
        data: json!({
            "name": "parent",
            "level": "dc",
            "region": "us-east-1",
            "meta": {
                "parent": "org:grandparent",
                "level": "dc",
                "region": "us-east-1"
            }
        }),
        data_sha: "parent_sha".to_string(),
        kids: Some(vec!["env:child".to_string()]),
    };

    Dim {
        dim_name: "child".to_string(),
        dim_type: "env".to_string(),
        key_path: PathBuf::from("org:grandparent/dc:parent/env:child"),
        dim_path: PathBuf::from("/test/inventory/test_org/env"),
        parent: Some(Box::new(parent)),
        data: json!({
            "name": "child",
            "level": "env",
            "environment": "staging",
            "meta": {
                "parent": "dc:parent",
                "level": "env",
                "environment": "staging"
            }
        }),
        data_sha: "child_sha".to_string(),
        kids: None,
    }
}

/// Create test directory structure with files
pub fn create_test_directory_structure() -> TempDir {
    let temp_dir = TempDir::new().unwrap();
    let base_path = temp_dir.path();
    
    // Create dimension directory structure
    let dim_path = base_path.join("test_org").join("test_type");
    std::fs::create_dir_all(&dim_path).unwrap();
    
    // Create test files with different prefixes using colon separator
    std::fs::write(
        dim_path.join(".default:config.yaml"),
        "default_config: value"
    ).unwrap();
    
    std::fs::write(
        dim_path.join("test_dim:config.yaml"),
        "dim_config: value"
    ).unwrap();
    
    std::fs::write(
        dim_path.join(".default:script.sh"),
        "#!/bin/bash\necho 'default script'"
    ).unwrap();
    
    std::fs::write(
        dim_path.join("test_dim:script.sh"),
        "#!/bin/bash\necho 'dim script'"
    ).unwrap();
    
    // Create a subdirectory with the correct prefix
    let sub_dir = dim_path.join(".default:subdir");
    std::fs::create_dir_all(&sub_dir).unwrap();
    std::fs::write(
        sub_dir.join("file.txt"),
        "subdirectory file"
    ).unwrap();
    
    temp_dir
}

/// Create test DimBuilder with mock data source
pub fn create_test_dim_builder() -> DimBuilder {
    DimBuilder {
        dim_name: "test_dim".to_string(),
        dim_type: "test_type".to_string(),
        org: "test_org".to_string(),
        dim_path: PathBuf::from("/test/inventory/test_org/test_type"),
        data: json!({
            "name": "test_dim",
            "region": "us-east-1",
            "meta": {
                "region": "us-east-1"
            }
        }),
        default_data: json!({
            "default_key": "default_value",
            "default_region": "us-west-1",
            "meta": {
                "default_region": "us-west-1"
            }
        }),
        datasource: data_src_init("test_org", "test_type", Storage::FS),
        storage: Storage::FS,
    }
}

/// Create test data for merging scenarios
pub fn create_merge_test_data() -> (Value, Value) {
    let default_data = json!({
        "string_key": "default_string",
        "number_key": 42,
        "bool_key": true,
        "array_key": ["default1", "default2"],
        "object_key": {
            "nested_string": "default_nested",
            "nested_number": 100,
            "default_only": "only_in_default"
        },
        "default_only_key": "default_only_value"
    });
    
    let override_data = json!({
        "string_key": "override_string",
        "number_key": 84,
        "array_key": ["override1", "override2", "override3"],
        "object_key": {
            "nested_string": "override_nested",
            "nested_bool": false,
            "override_only": "only_in_override"
        },
        "override_only_key": "override_only_value"
    });
    
    (default_data, override_data)
}

/// Assert that two JSON values are equivalent (ignoring order)
pub fn assert_json_equivalent(actual: &Value, expected: &Value) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            assert_eq!(a.len(), e.len(), "Object lengths differ");
            for (key, expected_value) in e {
                let actual_value = a.get(key).unwrap_or_else(|| panic!("Missing key: {}", key));
                assert_json_equivalent(actual_value, expected_value);
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            assert_eq!(a.len(), e.len(), "Array lengths differ");
            for (actual_item, expected_item) in a.iter().zip(e.iter()) {
                assert_json_equivalent(actual_item, expected_item);
            }
        }
        _ => assert_eq!(actual, expected, "Values differ"),
    }
}

/// Create test configuration for dimension relations
pub fn create_test_dim_relations() -> Vec<String> {
    vec![
        "org".to_string(),
        "dc".to_string(),
        "env".to_string(),
        "service".to_string(),
    ]
} 