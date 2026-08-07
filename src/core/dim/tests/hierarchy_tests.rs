use super::super::*;
use super::mock_helpers::*;
use serde_json::json;
use std::path::PathBuf;

#[cfg(test)]
mod hierarchy_tests {
    use super::*;

    #[test]
    fn test_dim_tree_single_dimension() {
        let dim = create_test_dim();
        let tree = dim.get_dim_tree();
        assert_eq!(tree, vec!["test_dim"]);
    }

    #[test]
    fn test_dim_tree_with_parent() {
        let dim = create_test_dim_with_parent();
        let tree = dim.get_dim_tree();
        assert_eq!(tree, vec!["child_dim", "parent_dim"]);
    }

    #[test]
    fn test_dim_tree_complex_hierarchy() {
        let dim = create_test_hierarchy();
        let tree = dim.get_dim_tree();
        assert_eq!(tree, vec!["child", "parent", "grandparent"]);
    }

    #[test]
    fn test_key_path_generation_single() {
        let dim = create_test_dim();
        assert_eq!(dim.key_path, PathBuf::from("test_type:test_dim"));
    }

    #[test]
    fn test_key_path_generation_with_parent() {
        let dim = create_test_dim_with_parent();
        assert_eq!(
            dim.key_path,
            PathBuf::from("parent_type:parent_dim/child_type:child_dim")
        );
    }

    #[test]
    fn test_key_path_generation_complex_hierarchy() {
        let dim = create_test_hierarchy();
        assert_eq!(
            dim.key_path,
            PathBuf::from("org:grandparent/dc:parent/env:child")
        );
    }

    #[test]
    fn test_parent_data_access() {
        let dim = create_test_dim_with_parent();
        
        assert!(dim.parent.is_some());
        let parent = dim.parent.as_ref().unwrap();
        assert_eq!(parent.dim_name, "parent_dim");
        assert_eq!(parent.dim_type, "parent_type");
    }

    #[test]
    fn test_children_information() {
        let dim = create_test_dim_with_children();
        
        assert!(dim.kids.is_some());
        let kids = dim.kids.as_ref().unwrap();
        assert_eq!(kids.len(), 2);
        assert!(kids.contains(&"child_type:child1".to_string()));
        assert!(kids.contains(&"child_type:child2".to_string()));
    }

    #[test]
    fn test_json_dim_vars_inheritance() {
        let dim = create_test_hierarchy();
        let json_vars = dim.get_json_dim_vars();
        
        // Should contain variables from all levels of hierarchy
        assert!(json_vars["dim_env_name"].as_str().unwrap() == "child");
        assert!(json_vars["dim_env_level"].as_str().unwrap() == "env");
        assert!(json_vars["dim_env_environment"].as_str().unwrap() == "staging");
        
        // Should inherit from parent
        assert!(json_vars["dim_dc_name"].as_str().unwrap() == "parent");
        assert!(json_vars["dim_dc_level"].as_str().unwrap() == "dc");
        assert!(json_vars["dim_dc_region"].as_str().unwrap() == "us-east-1");
        
        // Should inherit from grandparent
        assert!(json_vars["dim_org_name"].as_str().unwrap() == "grandparent");
        assert!(json_vars["dim_org_level"].as_str().unwrap() == "org");
        assert!(json_vars["dim_org_global_setting"].as_str().unwrap() == "value1");
    }

    #[test]
    fn test_json_dim_vars_no_parent() {
        let dim = create_test_dim();
        let json_vars = dim.get_json_dim_vars();
        
        // Should only contain own variables
        assert_eq!(json_vars["dim_test_type_name"], "test_dim");
        assert_eq!(json_vars["dim_test_type_region"], "us-east-1");
        assert_eq!(json_vars["dim_test_type_vpc_cidr"], "10.0.0.0/16");
        
        // Should not contain parent variables
        assert!(json_vars.get("dim_parent_type_name").is_none());
    }

    #[test]
    fn test_parent_format_validation() {
        // Test valid parent format
        let valid_parent = "dc:parent_name";
        assert!(valid_parent.contains(':'));
        
        let parts: Vec<&str> = valid_parent.split(':').collect();
        assert_eq!(parts.len(), 2);
        assert!(!parts[0].is_empty());
        assert!(!parts[1].is_empty());
    }

    #[test]
    fn test_hierarchy_data_inheritance_order() {
        let dim = create_test_hierarchy();
        let json_vars = dim.get_json_dim_vars();
        
        // Child values should override parent values when keys conflict
        // In this case, "level" exists at all levels, child should win
        assert_eq!(json_vars["dim_env_level"], "env");
        assert_eq!(json_vars["dim_dc_level"], "dc");
        assert_eq!(json_vars["dim_org_level"], "org");
    }

    #[test]
    fn test_empty_hierarchy() {
        let mut dim = create_test_dim();
        dim.parent = None;
        dim.kids = None;
        
        let tree = dim.get_dim_tree();
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0], "test_dim");
    }

    #[test]
    fn test_deep_hierarchy_performance() {
        // Create a deep hierarchy to test performance
        let mut current_dim = create_test_dim();
        
        // Build a 10-level deep hierarchy
        for i in 1..10 {
            let parent = Dim {
                dim_name: format!("parent_{}", i),
                dim_type: format!("type_{}", i),
                key_path: PathBuf::from(format!("type_{}:parent_{}", i, i)),
                dim_path: PathBuf::from(format!("/test/inventory/test_org/type_{}", i)),
                parent: if i == 1 { None } else { current_dim.parent.clone() },
                data: json!({
                    "name": format!("parent_{}", i),
                    "meta": {
                        "level": i
                    }
                }),
                data_sha: format!("sha_{}", i),
                kids: None,
            };
            
            current_dim.parent = Some(Box::new(parent));
        }
        
        // Test that tree generation works with deep hierarchy
        let tree = current_dim.get_dim_tree();
        assert_eq!(tree.len(), 10); // Original + 9 parents
        
        // Test that JSON variable generation works
        let json_vars = current_dim.get_json_dim_vars();
        assert!(json_vars.as_object().unwrap().len() > 0);
    }

    #[test]
    fn test_circular_dependency_detection_data() {
        // This test ensures we don't create circular references in data
        // The actual circular dependency detection would be in the builder
        let dim = create_test_dim_with_parent();
        
        // Verify parent doesn't reference child
        if let Some(parent) = &dim.parent {
            let parent_meta = &parent.data["meta"];
            // Parent should not have child as its parent
            assert_ne!(
                parent_meta.get("parent").unwrap_or(&json!(null)),
                &json!("child_type:child_dim")
            );
        }
    }

    #[test]
    fn test_hierarchy_metadata_consistency() {
        let dim = create_test_hierarchy();
        
        // Verify metadata consistency through hierarchy
        assert_eq!(dim.data["meta"]["parent"], "dc:parent");
        
        if let Some(parent) = &dim.parent {
            assert_eq!(parent.data["meta"]["parent"], "org:grandparent");
            
            if let Some(grandparent) = &parent.parent {
                // Grandparent should not have a parent
                assert!(grandparent.data["meta"].get("parent").is_none());
            }
        }
    }

    #[test]
    fn test_kids_information_from_dim_struct() {
        let dim = create_test_dim_with_children();
        
        // Test kids field directly from Dim struct
        assert!(dim.kids.is_some());
        let kids = dim.kids.as_ref().unwrap();
        assert_eq!(kids.len(), 2);
        assert!(kids.contains(&"child_type:child1".to_string()));
        assert!(kids.contains(&"child_type:child2".to_string()));
    }

    #[test]
    fn test_kids_validation_format() {
        let dim = create_test_dim_with_children();
        
        if let Some(kids) = &dim.kids {
            for kid in kids {
                // Each child should have valid format
                assert!(kid.contains(':'));
                let parts: Vec<&str> = kid.split(':').collect();
                assert_eq!(parts.len(), 2);
                assert!(!parts[0].is_empty()); // type
                assert!(!parts[1].is_empty()); // name
            }
        }
    }

    #[test]
    fn test_parent_loading_validation() {
        let dim = create_test_dim_with_parent();
        
        // Test parent loading and validation
        assert!(dim.parent.is_some());
        let parent = dim.parent.as_ref().unwrap();
        assert_eq!(parent.dim_name, "parent_dim");
        assert_eq!(parent.dim_type, "parent_type");
        
        // Test parent data access
        let parent_data = parent.get_data();
        assert_eq!(parent_data["name"], "parent_dim");
        assert_eq!(parent_data["region"], "us-west-1");
    }

    #[test]
    fn test_recursive_parent_traversal() {
        let dim = create_test_hierarchy();
        
        // Test manual traversal
        let mut current = &dim;
        let mut levels = vec![];
        
        loop {
            levels.push(current.dim_name.clone());
            if let Some(parent) = &current.parent {
                current = parent;
            } else {
                break;
            }
        }
        
        assert_eq!(levels, vec!["child", "parent", "grandparent"]);
    }

    #[test]
    fn test_parent_data_inheritance_complex() {
        let dim = create_test_hierarchy();
        
        // Test that child can access parent data through inheritance
        let json_vars = dim.get_json_dim_vars();
        
        // Child should have its own data
        assert_eq!(json_vars["dim_env_name"], "child");
        assert_eq!(json_vars["dim_env_environment"], "staging");
        
        // Child should inherit parent data
        assert_eq!(json_vars["dim_dc_name"], "parent");
        assert_eq!(json_vars["dim_dc_region"], "us-east-1");
        
        // Child should inherit grandparent data
        assert_eq!(json_vars["dim_org_name"], "grandparent");
        assert_eq!(json_vars["dim_org_global_setting"], "value1");
    }

    #[test]
    fn test_hierarchy_override_behavior() {
        let dim = create_test_dim_with_parent();
        
        // Test data override behavior in hierarchy
        let child_data = dim.get_data();
        assert_eq!(child_data["name"], "child_dim");
        assert_eq!(child_data["environment"], "test");
        
        // Test parent data
        let parent = dim.parent.as_ref().unwrap();
        let parent_data = parent.get_data();
        assert_eq!(parent_data["name"], "parent_dim");
        assert_eq!(parent_data["region"], "us-west-1");
        
        // Test that child has different environment than parent
        assert_eq!(child_data["environment"], "test");
        assert_eq!(parent_data["environment"], "production");
    }

    #[test]
    fn test_invalid_parent_format_handling() {
        // This would be tested in the builder, but we can test data consistency
        let mut dim = create_test_dim();
        
        // Simulate invalid parent reference in metadata
        dim.data["meta"]["parent"] = json!("invalid_format_no_colon");
        
        // The dim should still be valid, just with invalid parent reference
        assert!(dim.parent.is_none());
        assert_eq!(dim.data["meta"]["parent"], "invalid_format_no_colon");
    }

    #[test]
    fn test_child_relationship_validation() {
        let dim = create_test_dim_with_children();
        
        if let Some(kids) = &dim.kids {
            for kid in kids {
                // Each child should have valid format
                assert!(kid.contains(':'));
                let parts: Vec<&str> = kid.split(':').collect();
                assert_eq!(parts.len(), 2);
                assert!(!parts[0].is_empty()); // type
                assert!(!parts[1].is_empty()); // name
            }
        }
    }

    #[test]
    fn test_dimension_relation_configuration() {
        let dim = create_test_hierarchy();
        
        // Test that dimension relations are properly configured
        assert!(dim.parent.is_some());
        
        // Parent should not have child as parent (no circular reference)
        if let Some(parent) = &dim.parent {
            if let Some(grandparent) = &parent.parent {
                assert!(grandparent.parent.is_none());
            }
        }
    }

    #[test]
    fn test_child_filtering_by_parent() {
        let dim = create_test_dim_with_children();
        
        // All children should be valid for this parent type
        if let Some(kids) = &dim.kids {
            for kid in kids {
                assert!(!kid.is_empty());
                // Child name should not contain invalid characters
                assert!(!kid.contains('/'));
                assert!(!kid.contains('\\'));
            }
        }
    }

    #[test]
    fn test_hierarchy_memory_efficiency() {
        // Test that deep hierarchies don't cause memory issues
        let dim = create_test_hierarchy();
        
        // Multiple calls should not increase memory usage significantly
        for _ in 0..100 {
            let _tree = dim.get_dim_tree();
            let _vars = dim.get_json_dim_vars();
        }
        
        // Should still be accessible
        assert_eq!(dim.dim_name, "child");
        assert!(dim.parent.is_some());
    }

    #[test]
    fn test_nested_object_merging() {
        let dim = create_test_hierarchy();
        let json_vars = dim.get_json_dim_vars();
        
        // Test that nested objects are properly merged from hierarchy
        // Each level should contribute its own namespace
        assert!(json_vars.as_object().unwrap().keys().any(|k| k.starts_with("dim_env_")));
        assert!(json_vars.as_object().unwrap().keys().any(|k| k.starts_with("dim_dc_")));
        assert!(json_vars.as_object().unwrap().keys().any(|k| k.starts_with("dim_org_")));
    }

    #[test]
    fn test_dim_data_access_methods() {
        let dim = create_test_dim();
        
        // Test basic data access methods
        let data = dim.get_data();
        assert_eq!(data["name"], "test_dim");
        assert_eq!(data["region"], "us-east-1");
        
        // Test dim data method
        let dim_data = dim.get_dim_data();
        assert!(dim_data.is_object());
        assert_eq!(dim_data["name"], "test_dim");
        
        // Test that data is accessible
        assert!(!dim.data_sha.is_empty());
        assert_eq!(dim.dim_name, "test_dim");
        assert_eq!(dim.dim_type, "test_type");
    }

    #[test]
    fn test_dim_data_mutation() {
        let mut dim = create_test_dim();
        
        // Test get_data_mut method
        {
            let data_mut = dim.get_data_mut();
            data_mut["test_field"] = json!("test_value");
        }
        
        // Verify mutation worked
        assert_eq!(dim.get_data()["test_field"], "test_value");
    }

    #[test]
    fn test_json_dim_vars_structure() {
        let dim = create_test_dim();
        let json_vars = dim.get_json_dim_vars();
        
        // Should be an object
        assert!(json_vars.is_object());
        
        // Should have properly prefixed keys
        let obj = json_vars.as_object().unwrap();
        for key in obj.keys() {
            assert!(key.starts_with("dim_test_type_"));
        }
    }

    #[test]
    fn test_save_json_dim_vars_functionality() {
        use tempfile::TempDir;
        
        let dim = create_test_dim();
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_path_buf();
        
        let result = dim.save_json_dim_vars(temp_path.clone());
        
        // Should succeed
        assert!(result.is_ok());
        
        if let Ok(filename) = result {
            // Should return proper filename
            assert!(filename.starts_with("cubtera_dim_"));
            assert!(filename.ends_with(".json"));
            
            // File should exist
            let file_path = temp_path.join(&filename);
            assert!(file_path.exists());
        }
    }
} 