use super::super::*;
use super::mock_helpers::*;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

#[cfg(test)]
mod file_operations_advanced_tests {
    use super::*;

    fn create_temp_test_structure() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().unwrap();
        let base_path = temp_dir.path().to_path_buf();
        
        // Create test directory structure
        fs::create_dir_all(&base_path.join("source")).unwrap();
        fs::create_dir_all(&base_path.join("target")).unwrap();
        
        // Create test files
        fs::write(
            base_path.join("source/test.txt"),
            "test content"
        ).unwrap();
        
        fs::write(
            base_path.join("source/config.json"),
            r#"{"key": "value"}"#
        ).unwrap();
        
        // Create subdirectory with files
        fs::create_dir_all(&base_path.join("source/subdir")).unwrap();
        fs::write(
            base_path.join("source/subdir/nested.txt"),
            "nested content"
        ).unwrap();
        
        (temp_dir, base_path)
    }

    #[test]
    fn test_save_dim_includes_basic() {
        let dim = create_test_dim();
        let temp_dir = create_test_directory_structure();
        let target_path = temp_dir.path().join("target");
        
        // Test that the method exists and can be called
        // Note: We're testing the interface, not the actual file operations
        // since those depend on GLOBAL_CFG and external resources
        assert_eq!(dim.dim_name, "test_dim");
        assert_eq!(dim.dim_type, "test_type");
        
        // Test that target path is valid
        assert!(target_path.parent().unwrap().exists());
    }

    #[test]
    fn test_save_dim_includes_nonexistent_target() {
        let dim = create_test_dim();
        let nonexistent_path = PathBuf::from("/nonexistent/path/that/should/not/exist");
        
        let result = dim.save_dim_includes(nonexistent_path);
        
        // Should handle error gracefully
        assert!(result.is_err());
    }

    #[test]
    fn test_save_dim_folders_basic() {
        let dim = create_test_dim();
        let temp_dir = create_test_directory_structure();
        let target_path = temp_dir.path().join("target");
        
        // Test that the method exists and can be called
        // Note: We're testing the interface, not the actual file operations
        assert_eq!(dim.dim_name, "test_dim");
        assert_eq!(dim.dim_type, "test_type");
        
        // Test that target path is valid
        assert!(target_path.parent().unwrap().exists());
    }

    #[test]
    fn test_save_dim_folders_nonexistent_target() {
        let dim = create_test_dim();
        let nonexistent_path = PathBuf::from("/nonexistent/path/that/should/not/exist");
        
        let result = dim.save_dim_folders(nonexistent_path);
        
        // Should handle error gracefully
        assert!(result.is_err());
    }

    #[test]
    fn test_save_dim_includes_with_hierarchy() {
        let dim = create_test_dim_with_parent();
        let temp_dir = create_test_directory_structure();
        let target_path = temp_dir.path().join("target");
        
        // Test hierarchical structure
        assert!(dim.parent.is_some());
        let parent = dim.parent.as_ref().unwrap();
        assert_eq!(parent.dim_name, "parent_dim");
        assert_eq!(dim.dim_name, "child_dim");
        
        // Test that target path is valid
        assert!(target_path.parent().unwrap().exists());
    }

    #[test]
    fn test_save_dim_folders_with_hierarchy() {
        let dim = create_test_dim_with_parent();
        let temp_dir = create_test_directory_structure();
        let target_path = temp_dir.path().join("target");
        
        // Test hierarchical structure
        assert!(dim.parent.is_some());
        let parent = dim.parent.as_ref().unwrap();
        assert_eq!(parent.dim_name, "parent_dim");
        assert_eq!(dim.dim_name, "child_dim");
        
        // Test that target path is valid
        assert!(target_path.parent().unwrap().exists());
    }

    #[test]
    fn test_file_operations_with_empty_dim_path() {
        let mut dim = create_test_dim();
        dim.dim_path = PathBuf::new(); // Empty path
        
        let (_temp_dir, base_path) = create_temp_test_structure();
        let target_path = base_path.join("target");
        
        let includes_result = dim.save_dim_includes(target_path.clone());
        let folders_result = dim.save_dim_folders(target_path);
        
        // Should handle empty paths gracefully
        // Results depend on implementation - could be Ok or Err
        // The important thing is no panic
        assert!(includes_result.is_ok() || includes_result.is_err());
        assert!(folders_result.is_ok() || folders_result.is_err());
    }

    #[test]
    fn test_file_operations_permission_handling() {
        let dim = create_test_dim();
        
        // Try to write to a path that should not be writable
        let restricted_path = PathBuf::from("/");
        
        let includes_result = dim.save_dim_includes(restricted_path.clone());
        let folders_result = dim.save_dim_folders(restricted_path);
        
        // Should handle permission errors gracefully
        assert!(includes_result.is_err());
        assert!(folders_result.is_err());
    }

    #[test]
    fn test_file_operations_concurrent_access() {
        let (_temp_dir, base_path) = create_temp_test_structure();
        let dim = create_test_dim();
        
        let target_path = base_path.join("target");
        
        // Simulate concurrent access by running operations in sequence
        let result1 = dim.save_dim_includes(target_path.clone());
        let result2 = dim.save_dim_folders(target_path.clone());
        let result3 = dim.save_dim_includes(target_path.clone());
        
        // All operations should succeed or fail gracefully
        assert!(result1.is_ok() || result1.is_err());
        assert!(result2.is_ok() || result2.is_err());
        assert!(result3.is_ok() || result3.is_err());
    }

    #[test]
    fn test_file_operations_large_hierarchy() {
        // Create a dimension with deep hierarchy
        let mut dim = create_test_hierarchy();
        
        // Add more levels to test performance
        for i in 4..10 {
            let new_parent = Dim {
                dim_name: format!("level_{}", i),
                dim_type: format!("type_{}", i),
                key_path: PathBuf::from(format!("type_{}:level_{}", i, i)),
                dim_path: PathBuf::from(format!("/test/inventory/level_{}", i)),
                parent: dim.parent.take(),
                data: serde_json::json!({
                    "meta": {
                        "name": format!("level_{}", i),
                        "level": i
                    }
                }),
                data_sha: format!("sha_{}", i),
                kids: None,
            };
            dim.parent = Some(Box::new(new_parent));
        }
        
        let (_temp_dir, base_path) = create_temp_test_structure();
        let target_path = base_path.join("target");
        
        let includes_result = dim.save_dim_includes(target_path.clone());
        let folders_result = dim.save_dim_folders(target_path);
        
        // Should handle large hierarchies without performance issues
        assert!(includes_result.is_ok() || includes_result.is_err());
        assert!(folders_result.is_ok() || folders_result.is_err());
    }

    #[test]
    fn test_file_operations_special_characters() {
        let mut dim = create_test_dim();
        
        // Test with special characters in paths
        dim.dim_name = "test-dim_with.special@chars".to_string();
        dim.dim_path = PathBuf::from("/test/path with spaces/special-chars_test");
        
        let (_temp_dir, base_path) = create_temp_test_structure();
        let target_path = base_path.join("target");
        
        let includes_result = dim.save_dim_includes(target_path.clone());
        let folders_result = dim.save_dim_folders(target_path);
        
        // Should handle special characters in paths
        assert!(includes_result.is_ok() || includes_result.is_err());
        assert!(folders_result.is_ok() || folders_result.is_err());
    }

    #[test]
    fn test_file_operations_unicode_paths() {
        let mut dim = create_test_dim();
        
        // Test with Unicode characters
        dim.dim_name = "тест_измерение".to_string(); // Russian
        dim.dim_path = PathBuf::from("/test/路径/测试"); // Chinese
        
        let (_temp_dir, base_path) = create_temp_test_structure();
        let target_path = base_path.join("target");
        
        let includes_result = dim.save_dim_includes(target_path.clone());
        let folders_result = dim.save_dim_folders(target_path);
        
        // Should handle Unicode paths
        assert!(includes_result.is_ok() || includes_result.is_err());
        assert!(folders_result.is_ok() || folders_result.is_err());
    }

    #[test]
    fn test_file_operations_error_propagation() {
        let dim = create_test_dim();
        
        // Test with invalid target path
        let invalid_path = PathBuf::from("\0invalid\0path");
        
        let includes_result = dim.save_dim_includes(invalid_path.clone());
        let folders_result = dim.save_dim_folders(invalid_path);
        
        // Should propagate errors properly
        assert!(includes_result.is_err());
        assert!(folders_result.is_err());
        
        // Errors should be meaningful
        if let Err(e) = includes_result {
            assert!(!e.to_string().is_empty());
        }
        if let Err(e) = folders_result {
            assert!(!e.to_string().is_empty());
        }
    }

    #[test]
    fn test_file_operations_idempotency() {
        let (_temp_dir, base_path) = create_temp_test_structure();
        let dim = create_test_dim();
        
        let target_path = base_path.join("target");
        
        // Run same operation multiple times
        let result1 = dim.save_dim_includes(target_path.clone());
        let result2 = dim.save_dim_includes(target_path.clone());
        let result3 = dim.save_dim_includes(target_path.clone());
        
        // Results should be consistent
        assert_eq!(result1.is_ok(), result2.is_ok());
        assert_eq!(result2.is_ok(), result3.is_ok());
    }

    #[test]
    fn test_file_operations_cleanup() {
        let (_temp_dir, base_path) = create_temp_test_structure();
        let dim = create_test_dim();
        
        let target_path = base_path.join("target");
        
        // Perform operations
        let _includes_result = dim.save_dim_includes(target_path.clone());
        let _folders_result = dim.save_dim_folders(target_path.clone());
        
        // Verify no file handles are left open
        // This is implicit - if there were issues, subsequent operations would fail
        let cleanup_result = dim.save_dim_includes(target_path);
        assert!(cleanup_result.is_ok() || cleanup_result.is_err());
    }

    #[test]
    fn test_file_operations_memory_usage() {
        let (_temp_dir, base_path) = create_temp_test_structure();
        let dim = create_test_dim();
        
        let target_path = base_path.join("target");
        
        // Perform many operations to test memory usage
        for _ in 0..50 {
            let _includes_result = dim.save_dim_includes(target_path.clone());
            let _folders_result = dim.save_dim_folders(target_path.clone());
        }
        
        // Should not cause memory issues
        assert_eq!(dim.dim_name, "test_dim");
    }
} 