#[cfg(test)]
mod tests {
    use crate::core::runner::params::{RunnerParams, default_version, default_state_backend, default_lock_port};
    use std::collections::HashMap;

    #[test]
    fn test_runner_params_default() {
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
    fn test_runner_params_init_empty() {
        let empty_params = HashMap::new();
        let params = RunnerParams::init(empty_params);

        assert_eq!(params.version, "latest");
        assert_eq!(params.state_backend, "local");
        assert_eq!(params.lock_port, "65432");
    }

    #[test]
    fn test_runner_params_init_full() {
        let mut input_params = HashMap::new();
        input_params.insert("version".to_string(), "1.5.0".to_string());
        input_params.insert("state_backend".to_string(), "s3".to_string());
        input_params.insert("runner_command".to_string(), "terraform".to_string());
        input_params.insert("extra_args".to_string(), "--detailed-exitcode".to_string());
        input_params.insert("inlet_command".to_string(), "echo 'starting'".to_string());
        input_params.insert("outlet_command".to_string(), "echo 'finished'".to_string());
        input_params.insert("lock_port".to_string(), "8080".to_string());

        let params = RunnerParams::init(input_params);

        assert_eq!(params.version, "1.5.0");
        assert_eq!(params.state_backend, "s3");
        assert_eq!(params.runner_command, Some("terraform".to_string()));
        assert_eq!(params.extra_args, Some("--detailed-exitcode".to_string()));
        assert_eq!(params.inlet_command, Some("echo 'starting'".to_string()));
        assert_eq!(params.outlet_command, Some("echo 'finished'".to_string()));
        assert_eq!(params.lock_port, "8080");
    }

    #[test]
    fn test_runner_params_init_partial() {
        let mut input_params = HashMap::new();
        input_params.insert("version".to_string(), "1.3.0".to_string());
        input_params.insert("runner_command".to_string(), "tofu".to_string());

        let params = RunnerParams::init(input_params);

        assert_eq!(params.version, "1.3.0");
        assert_eq!(params.state_backend, "local"); // default
        assert_eq!(params.runner_command, Some("tofu".to_string()));
        assert!(params.extra_args.is_none());
        assert_eq!(params.lock_port, "65432"); // default
    }

    #[test]
    fn test_get_params_hashmap() {
        let params = RunnerParams {
            version: "1.4.0".to_string(),
            state_backend: "gcs".to_string(),
            runner_command: Some("terraform".to_string()),
            extra_args: Some("-auto-approve".to_string()),
            inlet_command: None,
            outlet_command: Some("cleanup.sh".to_string()),
            lock_port: "9090".to_string(),
        };

        let hashmap = params.get_params_hashmap();

        assert_eq!(hashmap.get("version"), Some(&"1.4.0".to_string()));
        assert_eq!(hashmap.get("state_backend"), Some(&"gcs".to_string()));
        assert_eq!(hashmap.get("lock_port"), Some(&"9090".to_string()));
        // Optional fields that are Some should be present
        assert!(hashmap.contains_key("runner_command"));
        assert!(hashmap.contains_key("extra_args"));
        assert!(hashmap.contains_key("outlet_command"));
    }

    #[test]
    fn test_get_lock_port_valid() {
        let mut params = RunnerParams::default();
        
        params.lock_port = "8080".to_string();
        assert_eq!(params.get_lock_port(), 8080);

        params.lock_port = "443".to_string();
        assert_eq!(params.get_lock_port(), 443);

        params.lock_port = "65535".to_string();
        assert_eq!(params.get_lock_port(), 65535);
    }

    #[test]
    fn test_get_lock_port_invalid() {
        let mut params = RunnerParams::default();
        
        params.lock_port = "invalid".to_string();
        assert_eq!(params.get_lock_port(), 65432); // fallback to default

        params.lock_port = "".to_string();
        assert_eq!(params.get_lock_port(), 65432);

        params.lock_port = "99999".to_string(); // out of range
        assert_eq!(params.get_lock_port(), 65432);

        params.lock_port = "-1".to_string();
        assert_eq!(params.get_lock_port(), 65432);
    }

    #[test]
    fn test_get_version() {
        let mut params = RunnerParams::default();
        assert_eq!(params.get_version(), "latest");

        params.version = "1.5.7".to_string();
        assert_eq!(params.get_version(), "1.5.7");

        params.version = "custom-build".to_string();
        assert_eq!(params.get_version(), "custom-build");
    }

    #[test]
    fn test_get_state_backend() {
        let mut params = RunnerParams::default();
        assert_eq!(params.get_state_backend(), "local");

        params.state_backend = "s3".to_string();
        assert_eq!(params.get_state_backend(), "s3");

        params.state_backend = "gcs".to_string();
        assert_eq!(params.get_state_backend(), "gcs");

        params.state_backend = "azurerm".to_string();
        assert_eq!(params.get_state_backend(), "azurerm");
    }

    #[test]
    fn test_serialization_deserialization() {
        let original_params = RunnerParams {
            version: "1.6.0".to_string(),
            state_backend: "consul".to_string(),
            runner_command: Some("terraform".to_string()),
            extra_args: Some("--parallelism=10".to_string()),
            inlet_command: Some("pre-hook.sh".to_string()),
            outlet_command: Some("post-hook.sh".to_string()),
            lock_port: "7777".to_string(),
        };

        // Serialize to JSON
        let json_value = serde_json::to_value(&original_params).unwrap();
        
        // Deserialize back
        let deserialized_params: RunnerParams = serde_json::from_value(json_value).unwrap();

        assert_eq!(original_params.version, deserialized_params.version);
        assert_eq!(original_params.state_backend, deserialized_params.state_backend);
        assert_eq!(original_params.runner_command, deserialized_params.runner_command);
        assert_eq!(original_params.extra_args, deserialized_params.extra_args);
        assert_eq!(original_params.inlet_command, deserialized_params.inlet_command);
        assert_eq!(original_params.outlet_command, deserialized_params.outlet_command);
        assert_eq!(original_params.lock_port, deserialized_params.lock_port);
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_version(), "latest");
        assert_eq!(default_state_backend(), "local");
        assert_eq!(default_lock_port(), "65432");
    }

    #[test]
    fn test_runner_params_with_special_characters() {
        let mut input_params = HashMap::new();
        input_params.insert("version".to_string(), "1.5.0-beta+build.123".to_string());
        input_params.insert("extra_args".to_string(), "--var='key=value with spaces'".to_string());
        input_params.insert("runner_command".to_string(), "/path/with spaces/terraform".to_string());

        let params = RunnerParams::init(input_params);

        assert_eq!(params.version, "1.5.0-beta+build.123");
        assert_eq!(params.extra_args, Some("--var='key=value with spaces'".to_string()));
        assert_eq!(params.runner_command, Some("/path/with spaces/terraform".to_string()));
    }

    #[test]
    fn test_runner_params_edge_cases() {
        let mut input_params = HashMap::new();
        input_params.insert("version".to_string(), "".to_string()); // empty version
        input_params.insert("lock_port".to_string(), "0".to_string()); // edge case port

        let params = RunnerParams::init(input_params);

        assert_eq!(params.version, ""); // should preserve empty string
        assert_eq!(params.get_lock_port(), 0); // should parse 0 correctly
    }

    #[test]
    fn test_runner_params_clone() {
        let original = RunnerParams {
            version: "1.5.0".to_string(),
            state_backend: "s3".to_string(),
            runner_command: Some("terraform".to_string()),
            extra_args: None,
            inlet_command: None,
            outlet_command: None,
            lock_port: "8080".to_string(),
        };

        let cloned = original.clone();

        assert_eq!(original.version, cloned.version);
        assert_eq!(original.state_backend, cloned.state_backend);
        assert_eq!(original.runner_command, cloned.runner_command);
        assert_eq!(original.lock_port, cloned.lock_port);
    }

    #[test]
    fn test_runner_params_debug() {
        let params = RunnerParams::default();
        let debug_string = format!("{:?}", params);
        
        assert!(debug_string.contains("RunnerParams"));
        assert!(debug_string.contains("version"));
        assert!(debug_string.contains("state_backend"));
    }

    #[test]
    fn test_large_hashmap_conversion() {
        let mut large_params = HashMap::new();
        for i in 0..1000 {
            large_params.insert(format!("custom_param_{}", i), format!("value_{}", i));
        }
        large_params.insert("version".to_string(), "test".to_string());

        let params = RunnerParams::init(large_params);
        let converted_back = params.get_params_hashmap();

        assert_eq!(params.version, "test");
        // The converted_back only contains the standard RunnerParams fields, not custom ones
        assert!(converted_back.len() >= 3); // At least version, state_backend, lock_port
    }

    #[test]
    fn test_runner_params_with_unicode() {
        let mut input_params = HashMap::new();
        input_params.insert("version".to_string(), "1.5.0-ñ".to_string());
        input_params.insert("extra_args".to_string(), "--var='测试=тест'".to_string());

        let params = RunnerParams::init(input_params);

        assert_eq!(params.version, "1.5.0-ñ");
        assert_eq!(params.extra_args, Some("--var='测试=тест'".to_string()));
    }
} 