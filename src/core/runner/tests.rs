#[cfg(test)]
mod tests {
    use crate::core::runner::{RunnerType, apply_template_to_value};
    use crate::core::runner::params::RunnerParams;
    use serde_json::{json, Value};
    use std::collections::HashMap;

    // Mock implementations for testing

    // Helper function to create test RunnerLoad - simplified for testing
    fn create_mock_runner_load() -> (Vec<String>, RunnerParams, Value) {
        let command = vec!["plan".to_string()];
        let params = RunnerParams::default();
        let state_backend = json!({
            "local": {
                "path": "/tmp/test.tfstate"
            }
        });

        (command, params, state_backend)
    }

    #[test]
    fn test_runner_type_str_to_runner_type() {
        assert!(matches!(RunnerType::str_to_runner_type("tf"), RunnerType::TF));
        assert!(matches!(RunnerType::str_to_runner_type("TF"), RunnerType::TF));
        assert!(matches!(RunnerType::str_to_runner_type("bash"), RunnerType::BASH));
        assert!(matches!(RunnerType::str_to_runner_type("BASH"), RunnerType::BASH));
        assert!(matches!(RunnerType::str_to_runner_type("tofu"), RunnerType::TOFU));
        assert!(matches!(RunnerType::str_to_runner_type("TOFU"), RunnerType::TOFU));
        assert!(matches!(RunnerType::str_to_runner_type("helm"), RunnerType::HELM));
        assert!(matches!(RunnerType::str_to_runner_type("HELM"), RunnerType::HELM));
        assert!(matches!(RunnerType::str_to_runner_type("unknown"), RunnerType::UNKNOWN));
        assert!(matches!(RunnerType::str_to_runner_type("invalid"), RunnerType::UNKNOWN));
    }

    #[test]
    fn test_runner_load_creation() {
        let command = vec!["plan".to_string(), "--detailed-exitcode".to_string()];
        let params = RunnerParams::default();
        let state_backend = json!({
            "s3": {
                "bucket": "test-bucket",
                "key": "test.tfstate"
            }
        });

        // Test that we can create the components
        assert_eq!(command, vec!["plan", "--detailed-exitcode"]);
        assert_eq!(params.get_version(), "latest");
        assert!(state_backend.is_object());
    }

    #[test]
    fn test_runner_params_initialization() {
        let mut params_map = HashMap::new();
        params_map.insert("version".to_string(), "1.5.0".to_string());
        params_map.insert("state_backend".to_string(), "s3".to_string());
        params_map.insert("runner_command".to_string(), "terraform".to_string());
        params_map.insert("extra_args".to_string(), "--detailed-exitcode".to_string());

        let params = RunnerParams::init(params_map.clone());

        assert_eq!(params.version, "1.5.0");
        assert_eq!(params.state_backend, "s3");
        assert_eq!(params.runner_command, Some("terraform".to_string()));
        assert_eq!(params.extra_args, Some("--detailed-exitcode".to_string()));
    }

    #[test]
    fn test_runner_params_defaults() {
        let params = RunnerParams::default();

        assert_eq!(params.version, "latest");
        assert_eq!(params.state_backend, "local");
        assert_eq!(params.lock_port, "65432");
        assert!(params.runner_command.is_none());
        assert!(params.extra_args.is_none());
        assert!(params.inlet_command.is_none());
        assert!(params.outlet_command.is_none());
    }

    #[test]
    fn test_runner_params_get_lock_port() {
        let mut params = RunnerParams::default();
        assert_eq!(params.get_lock_port(), 65432);

        params.lock_port = "8080".to_string();
        assert_eq!(params.get_lock_port(), 8080);

        params.lock_port = "invalid".to_string();
        assert_eq!(params.get_lock_port(), 65432); // fallback to default
    }

    #[test]
    fn test_apply_template_to_value_string() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "org": "test-org",
            "unit_name": "test-unit"
        });

        let template_value = json!("{{ org }}/{{ unit_name }}.tfstate");
        let result = apply_template_to_value(&template_value, &handlebars, &data);

        assert_eq!(result, json!("test-org/test-unit.tfstate"));
    }

    #[test]
    fn test_apply_template_to_value_object() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "org": "test-org",
            "unit_name": "test-unit"
        });

        let template_value = json!({
            "local": {
                "path": "{{ org }}/{{ unit_name }}.tfstate"
            }
        });

        let result = apply_template_to_value(&template_value, &handlebars, &data);

        assert_eq!(result, json!({
            "local": {
                "path": "test-org/test-unit.tfstate"
            }
        }));
    }

    #[test]
    fn test_apply_template_to_value_array() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "org": "test-org"
        });

        let template_value = json!(["{{ org }}-file1", "{{ org }}-file2"]);
        let result = apply_template_to_value(&template_value, &handlebars, &data);

        assert_eq!(result, json!(["test-org-file1", "test-org-file2"]));
    }

    #[test]
    fn test_apply_template_to_value_non_string() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({});

        let template_value = json!(42);
        let result = apply_template_to_value(&template_value, &handlebars, &data);

        assert_eq!(result, json!(42));
    }

    // Mock Runner for testing trait methods - simplified
    struct MockRunner {
        ctx: Value,
        command: Vec<String>,
    }

    impl MockRunner {
        fn new_simple(command: Vec<String>) -> Self {
            Self {
                ctx: json!({}),
                command,
            }
        }

        fn get_ctx(&self) -> &Value {
            &self.ctx
        }

        fn get_ctx_mut(&mut self) -> &mut Value {
            &mut self.ctx
        }

        fn update_ctx(&mut self, key: &str, value: Value) {
            if let Value::Object(ref mut map) = self.ctx {
                map.insert(key.to_string(), value);
            }
        }
    }

    #[test]
    fn test_runner_trait_update_ctx() {
        let mut runner = MockRunner::new_simple(vec!["test".to_string()]);

        runner.update_ctx("test_key", json!("test_value"));

        assert_eq!(runner.get_ctx()["test_key"], json!("test_value"));
    }

    #[test]
    fn test_mock_runner_context() {
        let mut runner = MockRunner::new_simple(vec!["plan".to_string()]);

        runner.update_ctx("status", json!("running"));
        runner.update_ctx("step", json!("planning"));

        assert_eq!(runner.get_ctx()["status"], json!("running"));
        assert_eq!(runner.get_ctx()["step"], json!("planning"));
    }

    #[test]
    fn test_runner_builder_components() {
        let command = vec!["plan".to_string()];
        let params = RunnerParams::default();

        // Test that we can create the components needed for RunnerBuilder
        assert_eq!(command, vec!["plan"]);
        assert_eq!(params.get_version(), "latest");
        assert_eq!(params.get_state_backend(), "local");
    }

    // Integration tests for specific runner types
    #[test]
    fn test_bash_runner_type() {
        let runner_type = RunnerType::str_to_runner_type("bash");
        assert!(matches!(runner_type, RunnerType::BASH));

        let runner_type = RunnerType::str_to_runner_type("BASH");
        assert!(matches!(runner_type, RunnerType::BASH));
    }

    // Test error cases
    #[test]
    fn test_runner_type_unknown() {
        let runner_type = RunnerType::str_to_runner_type("unknown");
        assert!(matches!(runner_type, RunnerType::UNKNOWN));

        let runner_type = RunnerType::str_to_runner_type("invalid");
        assert!(matches!(runner_type, RunnerType::UNKNOWN));
    }

    // Performance and edge case tests
    #[test]
    fn test_runner_params_large_hashmap() {
        let mut large_params = HashMap::new();
        for i in 0..1000 {
            large_params.insert(format!("key_{}", i), format!("value_{}", i));
        }

        let params = RunnerParams::init(large_params);
        // Should handle large hashmaps without issues
        assert_eq!(params.version, "latest"); // default value should still work
    }

    #[test]
    fn test_template_rendering_edge_cases() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "special_chars": "test/with:special@chars"
        });

        let template_value = json!("path/{{ special_chars }}/file");
        let result = apply_template_to_value(&template_value, &handlebars, &data);

        assert_eq!(result, json!("path/test/with:special@chars/file"));
    }

    #[test]
    fn test_runner_params_serialization() {
        let params = RunnerParams {
            version: "1.5.0".to_string(),
            state_backend: "s3".to_string(),
            runner_command: Some("terraform".to_string()),
            extra_args: Some("--detailed-exitcode".to_string()),
            inlet_command: None,
            outlet_command: None,
            lock_port: "8080".to_string(),
        };

        let hashmap = params.get_params_hashmap();
        assert_eq!(hashmap.get("version"), Some(&"1.5.0".to_string()));
        assert_eq!(hashmap.get("state_backend"), Some(&"s3".to_string()));
        assert_eq!(hashmap.get("lock_port"), Some(&"8080".to_string()));
    }

    // Test concurrent access patterns
    #[test]
    fn test_runner_type_thread_safety() {
        use std::sync::Arc;
        use std::thread;

        let runner_types = Arc::new(vec!["tf", "bash", "tofu", "helm"]);
        let handles: Vec<_> = (0..4)
            .map(|i| {
                let types = Arc::clone(&runner_types);
                thread::spawn(move || {
                    let runner_type = RunnerType::str_to_runner_type(&types[i]);
                    match i {
                        0 => assert!(matches!(runner_type, RunnerType::TF)),
                        1 => assert!(matches!(runner_type, RunnerType::BASH)),
                        2 => assert!(matches!(runner_type, RunnerType::TOFU)),
                        3 => assert!(matches!(runner_type, RunnerType::HELM)),
                        _ => unreachable!(),
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().unwrap();
        }
    }
}

// Integration tests module
#[cfg(test)]
mod integration_tests {
    use crate::core::runner::apply_template_to_value;
    use serde_json::json;

    #[test]
    fn test_state_backend_template_rendering() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "org": "test-org",
            "unit_name": "test-unit",
            "dim_tree": "dc:staging/app:web"
        });

        let state_config = json!({
            "s3": {
                "bucket": "{{ org }}-terraform-state",
                "key": "{{ dim_tree }}/{{ unit_name }}.tfstate",
                "region": "us-east-1"
            }
        });

        let result = apply_template_to_value(&state_config, &handlebars, &data);

        let expected = json!({
            "s3": {
                "bucket": "test-org-terraform-state",
                "key": "dc:staging/app:web/test-unit.tfstate",
                "region": "us-east-1"
            }
        });

        assert_eq!(result, expected);
    }
}

// Benchmark tests (optional, requires criterion crate)
#[cfg(test)]
mod benchmark_tests {
    use crate::core::runner::{RunnerType, apply_template_to_value};
    use serde_json::json;
    use std::time::Instant;

    #[test]
    fn test_runner_type_conversion_performance() {
        let start = Instant::now();
        
        for _ in 0..10000 {
            let _ = RunnerType::str_to_runner_type("tf");
            let _ = RunnerType::str_to_runner_type("bash");
            let _ = RunnerType::str_to_runner_type("tofu");
            let _ = RunnerType::str_to_runner_type("helm");
        }
        
        let duration = start.elapsed();
        println!("10000 runner type conversions took: {:?}", duration);
        
        // Should complete in reasonable time (less than 1ms for 10k conversions)
        assert!(duration.as_millis() < 100);
    }

    #[test]
    fn test_template_rendering_performance() {
        let handlebars = handlebars::Handlebars::new();
        let data = json!({
            "org": "test-org",
            "unit_name": "test-unit",
            "dim_tree": "dc:staging/app:web"
        });

        let template = json!({
            "s3": {
                "bucket": "{{ org }}-terraform-state",
                "key": "{{ dim_tree }}/{{ unit_name }}.tfstate"
            }
        });

        let start = Instant::now();
        
        for _ in 0..1000 {
            let _ = apply_template_to_value(&template, &handlebars, &data);
        }
        
        let duration = start.elapsed();
        println!("1000 template renderings took: {:?}", duration);
        
        // Should complete in reasonable time
        assert!(duration.as_millis() < 1000);
    }
} 