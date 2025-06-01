#[cfg(test)]
mod tests {
    use crate::tools::string::*;
    use std::env;
    use std::path::PathBuf;

    #[test]
    fn test_capitalize_first() {
        assert_eq!(capitalize_first("hello"), "Hello");
        assert_eq!(capitalize_first(""), "");
        assert_eq!(capitalize_first("a"), "A");
        assert_eq!(capitalize_first("HELLO"), "HELLO");
        assert_eq!(capitalize_first("123abc"), "123abc");
        assert_eq!(capitalize_first("мир"), "Мир"); // Unicode test
    }

    #[test]
    fn test_string_to_path_simple() {
        let result = string_to_path("/usr/bin").unwrap();
        assert_eq!(result, PathBuf::from("/usr/bin"));
    }

    #[test]
    fn test_string_to_path_empty() {
        let result = string_to_path("").unwrap();
        assert_eq!(result, PathBuf::from(""));
    }

    #[test]
    fn test_string_to_path_tilde_expansion() {
        // Set HOME for test
        env::set_var("HOME", "/home/testuser");
        
        let result = string_to_path("~/Documents").unwrap();
        assert_eq!(result, PathBuf::from("/home/testuser/Documents"));
        
        let result = string_to_path("~/.config/app").unwrap();
        assert_eq!(result, PathBuf::from("/home/testuser/.config/app"));
    }

    #[test]
    fn test_string_to_path_tilde_expansion_no_home() {
        // Remove HOME variable
        env::remove_var("HOME");
        
        let result = string_to_path("~/Documents");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HOME environment variable not found"));
    }

    #[test]
    fn test_string_to_path_env_var_expansion() {
        env::set_var("TEST_VAR", "test_value");
        env::set_var("ANOTHER_VAR", "another_value");
        
        let result = string_to_path("/$TEST_VAR/path").unwrap();
        assert_eq!(result, PathBuf::from("/test_value/path"));
        
        let result = string_to_path("/${TEST_VAR}/path").unwrap();
        assert_eq!(result, PathBuf::from("/test_value/path"));
        
        let result = string_to_path("/$TEST_VAR/$ANOTHER_VAR").unwrap();
        assert_eq!(result, PathBuf::from("/test_value/another_value"));
    }

    #[test]
    fn test_string_to_path_relative_expansion() {
        let current_dir = env::current_dir().unwrap();
        
        let result = string_to_path("./relative/path").unwrap();
        let expected = current_dir.join("relative/path");
        assert_eq!(result, expected);
    }

    #[test]
    fn test_string_to_path_complex() {
        env::set_var("HOME", "/home/testuser");
        env::set_var("USER", "testuser");
        env::set_var("PROJECT", "myproject");
        
        let result = string_to_path("~/Documents/${USER}/$PROJECT/src").unwrap();
        assert_eq!(result, PathBuf::from("/home/testuser/Documents/testuser/myproject/src"));
    }

    #[test]
    fn test_convert_path_to_absolute_tilde() {
        env::set_var("HOME", "/home/testuser");
        
        let result = convert_path_to_absolute("~/Documents".to_string()).unwrap();
        assert_eq!(result, "/home/testuser/Documents");
    }

    #[test]
    fn test_convert_path_to_absolute_relative() {
        let current_dir = env::current_dir().unwrap();
        
        let result = convert_path_to_absolute("./relative".to_string()).unwrap();
        let expected = current_dir.join("relative").to_string_lossy().to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_path_to_absolute_absolute() {
        let result = convert_path_to_absolute("/absolute/path".to_string()).unwrap();
        assert_eq!(result, "/absolute/path");
    }

    #[test]
    fn test_convert_path_to_absolute_relative_path() {
        let current_dir = env::current_dir().unwrap();
        
        let result = convert_path_to_absolute("relative/path".to_string()).unwrap();
        let expected = current_dir.join("relative/path").to_string_lossy().to_string();
        assert_eq!(result, expected);
    }

    #[test]
    fn test_convert_path_to_absolute_no_home() {
        env::remove_var("HOME");
        
        let result = convert_path_to_absolute("~/Documents".to_string());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HOME environment variable not found"));
    }

    #[test]
    fn test_edge_cases() {
        // Test with special characters
        env::set_var("SPECIAL_VAR", "value with spaces");
        let result = string_to_path("/$SPECIAL_VAR/path").unwrap();
        assert_eq!(result, PathBuf::from("/value with spaces/path"));
        
        // Test with Unicode
        env::set_var("UNICODE_VAR", "значение");
        let result = string_to_path("/$UNICODE_VAR/путь").unwrap();
        assert_eq!(result, PathBuf::from("/значение/путь"));
    }

    #[test]
    fn test_nonexistent_env_var() {
        // Non-existent env vars should remain as-is
        let result = string_to_path("/$NONEXISTENT_VAR/path").unwrap();
        assert_eq!(result, PathBuf::from("/$NONEXISTENT_VAR/path"));
    }

    #[test]
    fn test_tilde_in_middle() {
        // Tilde should only expand at the beginning
        let result = string_to_path("/path/~/other").unwrap();
        assert_eq!(result, PathBuf::from("/path/~/other"));
    }

    // Performance test
    #[test]
    fn test_performance_string_to_path() {
        use std::time::Instant;
        
        env::set_var("PERF_VAR", "performance_test");
        let start = Instant::now();
        
        for _ in 0..1000 {
            let _ = string_to_path("/$PERF_VAR/test/path").unwrap();
        }
        
        let duration = start.elapsed();
        // Should complete in reasonable time (less than 100ms for 1000 operations)
        assert!(duration.as_millis() < 100, "Performance test took too long: {:?}", duration);
    }

    // Cleanup after tests
    impl Drop for TestCleanup {
        fn drop(&mut self) {
            // Clean up test environment variables
            env::remove_var("TEST_VAR");
            env::remove_var("ANOTHER_VAR");
            env::remove_var("USER");
            env::remove_var("PROJECT");
            env::remove_var("SPECIAL_VAR");
            env::remove_var("UNICODE_VAR");
            env::remove_var("PERF_VAR");
        }
    }

    struct TestCleanup;

    #[test]
    fn _cleanup() {
        let _cleanup = TestCleanup;
    }
} 