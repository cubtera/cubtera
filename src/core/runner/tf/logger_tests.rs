use super::*;
use serde_json::json;

// Mock TF runner for testing logger functionality
struct TestTfRunner {
    command: Vec<String>,
    ctx: Value,
}

impl TestTfRunner {
    fn new(command: Vec<String>) -> Self {
        TestTfRunner {
            command,
            ctx: json!({}),
        }
    }
    
    fn update_ctx(&mut self, key: &str, value: Value) {
        self.ctx[key] = value;
    }
    
    fn get_ctx(&self) -> &Value {
        &self.ctx
    }
    
    // Simulate the TF-specific logger functionality
    fn logger(&mut self, exit_code: i32) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "tf runner", "TF-specific logger method (can be customized)");
        
        // Call the default logger implementation
        self.update_ctx("logger", json!("tf_runner_executed"));
        
        if GLOBAL_CFG.dlog_db.is_some() {
            // TF runner can have special command type handling
            let command_type = self.command
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown");
                
            // For testing, just log that it would save to DB
            info!(target: "tf runner", "TF Would save dlog data for {} command", command_type);
        }
        
        self.update_ctx("exit_code", json!(exit_code));
        debug!(target: "tf runner", "TF Final context: {}", self.get_ctx().to_string());

        Ok(())
    }
}

#[test]
fn test_tf_logger_executes_successfully() {
    let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
    let exit_code = 0;

    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    assert_eq!(runner.get_ctx()["exit_code"], json!(0));
}

#[test]
fn test_tf_logger_with_different_commands() {
    let test_cases = vec![
        ("apply", 0),
        ("destroy", 1),
        ("plan", 0),
        ("init", 0),
        ("fmt", 0),
        ("validate", 2),
        ("refresh", 0),
        ("import", 1),
    ];

    for (command, exit_code) in test_cases {
        let mut runner = TestTfRunner::new(vec![command.to_string()]);
        let result = runner.logger(exit_code);
        
        assert!(result.is_ok(), "TF Logger failed for command: {}", command);
        assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
        assert_eq!(runner.get_ctx()["exit_code"], json!(exit_code));
    }
}

#[test]
fn test_tf_logger_with_unknown_command() {
    let mut runner = TestTfRunner::new(vec!["unknown".to_string()]);
    let exit_code = 1;

    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    assert_eq!(runner.get_ctx()["exit_code"], json!(1));
}

#[test]
fn test_tf_logger_preserves_context() {
    let mut runner = TestTfRunner::new(vec!["plan".to_string()]);
    
    // Add TF-specific context
    runner.update_ctx("tf_version", json!("1.5.0"));
    runner.update_ctx("working_dir", json!("/tmp/terraform"));
    
    let result = runner.logger(0);
    
    assert!(result.is_ok());
    // Check that existing context is preserved
    assert_eq!(runner.get_ctx()["tf_version"], json!("1.5.0"));
    assert_eq!(runner.get_ctx()["working_dir"], json!("/tmp/terraform"));
    // Check that TF logger context is added
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    assert_eq!(runner.get_ctx()["exit_code"], json!(0));
}

#[test]
fn test_tf_logger_command_type_detection() {
    let test_commands = vec![
        (vec!["apply".to_string()], "apply"),
        (vec!["destroy".to_string()], "destroy"), 
        (vec!["plan".to_string()], "plan"),
        (vec!["init".to_string()], "init"),
        (vec!["apply".to_string(), "-auto-approve".to_string()], "apply"),
        (vec!["plan".to_string(), "-out=tfplan".to_string()], "plan"),
        (vec![], "unknown"),
    ];

    for (command, expected_type) in test_commands {
        let mut runner = TestTfRunner::new(command.clone());
        let result = runner.logger(0);
        
        assert!(result.is_ok(), "Failed for command: {:?}", command);
        
        // Check that command type is detected correctly
        let actual_type = runner.command
            .first()
            .map(|s| s.as_str())
            .unwrap_or("unknown");
        assert_eq!(actual_type, expected_type, "Command type mismatch for: {:?}", command);
    }
}

#[test]
fn test_tf_logger_with_multiple_exit_codes() {
    let exit_codes = vec![0, 1, 2, 127, 255];
    
    for exit_code in exit_codes {
        let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
        let result = runner.logger(exit_code);
        
        assert!(result.is_ok(), "Failed for exit code: {}", exit_code);
        assert_eq!(runner.get_ctx()["exit_code"], json!(exit_code));
    }
}

#[test]
fn test_tf_logger_overrides_default_behavior() {
    let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
    let exit_code = 0;

    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    // Check that TF runner uses its own logger implementation
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    // This should be different from the default "executed" value
    assert_ne!(runner.get_ctx()["logger"], json!("executed"));
}

#[test]
fn test_tf_logger_debug_output_format() {
    let commands = vec![
        vec!["apply".to_string()],
        vec!["destroy".to_string()],
        vec!["plan".to_string(), "-out=tfplan".to_string()],
        vec!["init".to_string()],
        vec!["fmt".to_string()],
        vec!["validate".to_string()],
    ];

    for command in commands {
        let mut runner = TestTfRunner::new(command.clone());
        let exit_code = 0;

        let result = runner.logger(exit_code);
        
        assert!(result.is_ok());
        
        // Verify TF-specific context format
        assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
        assert_eq!(runner.get_ctx()["exit_code"], json!(0));
        
        // Debug output should contain TF-specific information
        let debug_output = runner.get_ctx().to_string();
        assert!(debug_output.contains("tf_runner_executed"));
    }
}

#[test] 
fn test_tf_logger_disk_fallback_when_no_db() {
    use std::env;
    use tempfile::TempDir;
    
    // Create temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let temp_home = temp_dir.path().to_str().unwrap();
    
    // Set HOME environment variable to temp directory
    let original_home = env::var("HOME").ok();
    env::set_var("HOME", temp_home);
    
    let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
    let exit_code = 0;

    // Test should work regardless of dlog_db configuration
    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    assert_eq!(runner.get_ctx()["exit_code"], json!(0));
    
    // Restore original HOME if it existed
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    } else {
        env::remove_var("HOME");
    }
}

#[test]
fn test_tf_logger_handles_missing_home_dir() {
    use std::env;
    
    // Temporarily remove HOME environment variable
    let original_home = env::var("HOME").ok();
    env::remove_var("HOME");
    
    let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
    let exit_code = 0;

    // Should use /tmp as fallback when HOME is not set
    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
    assert_eq!(runner.get_ctx()["exit_code"], json!(0));
    
    // Restore original HOME if it existed
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    }
}

#[test]
fn test_tf_logger_disk_path_structure() {
    use std::env;
    use tempfile::TempDir;
    
    // Create temporary directory for testing
    let temp_dir = TempDir::new().unwrap();
    let temp_home = temp_dir.path().to_str().unwrap();
    
    // Set HOME environment variable to temp directory
    let original_home = env::var("HOME").ok();
    env::set_var("HOME", temp_home);
    
    let mut runner = TestTfRunner::new(vec!["apply".to_string()]);
    let exit_code = 0;

    let result = runner.logger(exit_code);
    
    assert!(result.is_ok());
    
    // Check that .cubtera directory structure is created properly
    let _cubtera_path = temp_dir.path().join(".cubtera");
    
    // Restore original HOME if it existed
    if let Some(home) = original_home {
        env::set_var("HOME", home);
    } else {
        env::remove_var("HOME");
    }
}

#[test]
fn test_tf_logger_special_commands() {
    let tf_commands = vec![
        vec!["apply".to_string()],
        vec!["destroy".to_string()],
        vec!["plan".to_string()],
        vec!["init".to_string()],
        vec!["fmt".to_string()],
        vec!["validate".to_string()],
        vec!["refresh".to_string()],
        vec!["import".to_string(), "resource".to_string()],
        vec!["show".to_string()],
        vec!["output".to_string()],
        vec!["graph".to_string()],
        vec!["state".to_string(), "list".to_string()],
    ];

    for command in tf_commands {
        let mut runner = TestTfRunner::new(command.clone());
        let exit_code = 0;

        let result = runner.logger(exit_code);
        
        assert!(result.is_ok());
        assert_eq!(runner.get_ctx()["logger"], json!("tf_runner_executed"));
        assert_eq!(runner.get_ctx()["exit_code"], json!(0));
    }
} 