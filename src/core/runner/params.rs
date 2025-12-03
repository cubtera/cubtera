use crate::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RunnerParams {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_state_backend")]
    pub state_backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_args: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inlet_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlet_command: Option<String>,
    #[serde(default = "default_lock_port")]
    pub lock_port: String,
}

#[allow(dead_code)]
impl RunnerParams {
    pub fn init(params: HashMap<String, String>) -> Self {
        let value =
            serde_json::to_value(params).unwrap_or_exit("Failed to convert runner params".into());
        serde_json::from_value::<RunnerParams>(value)
            .unwrap_or_exit("Failed to convert runner params".into())
    }

    pub fn get_params_hashmap(&self) -> HashMap<String, String> {
        let value = serde_json::to_value(self).unwrap_or_default();
        serde_json::from_value::<HashMap<String, String>>(value).unwrap_or_default()
    }

    pub fn get_lock_port(&self) -> u16 {
        self.lock_port.parse().unwrap_or(65432)
    }

    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    pub fn get_state_backend(&self) -> String {
        self.state_backend.clone()
    }
}

fn default_lock_port() -> String {
    String::from("65432")
}

fn default_version() -> String {
    String::from("latest")
}

fn default_state_backend() -> String {
    String::from("local")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to create params with serde defaults applied (via init from empty hashmap)
    fn create_params_with_defaults() -> RunnerParams {
        RunnerParams::init(HashMap::new())
    }

    #[test]
    fn test_rust_default_values() {
        // Rust's Default trait gives empty strings, not serde defaults
        let params = RunnerParams::default();

        // These will be empty strings due to Rust's Default, not serde defaults
        assert_eq!(params.version, "");
        assert_eq!(params.state_backend, "");
        assert_eq!(params.lock_port, "");
        assert!(params.runner_command.is_none());
        assert!(params.extra_args.is_none());
        assert!(params.inlet_command.is_none());
        assert!(params.outlet_command.is_none());
    }

    #[test]
    fn test_serde_default_values() {
        // Serde defaults are applied during deserialization
        let params = create_params_with_defaults();

        assert_eq!(params.version, "latest");
        assert_eq!(params.state_backend, "local");
        assert_eq!(params.lock_port, "65432");
        assert!(params.runner_command.is_none());
        assert!(params.extra_args.is_none());
        assert!(params.inlet_command.is_none());
        assert!(params.outlet_command.is_none());
    }

    #[test]
    fn test_init_from_empty_hashmap() {
        let params_map: HashMap<String, String> = HashMap::new();
        let params = RunnerParams::init(params_map);

        assert_eq!(params.version, "latest");
        assert_eq!(params.state_backend, "local");
        assert_eq!(params.lock_port, "65432");
    }

    #[test]
    fn test_init_from_hashmap_with_values() {
        let mut params_map: HashMap<String, String> = HashMap::new();
        params_map.insert("version".to_string(), "1.5.0".to_string());
        params_map.insert("state_backend".to_string(), "s3".to_string());
        params_map.insert("lock_port".to_string(), "12345".to_string());
        params_map.insert("runner_command".to_string(), "terraform".to_string());
        params_map.insert("extra_args".to_string(), "-json".to_string());
        params_map.insert("inlet_command".to_string(), "echo inlet".to_string());
        params_map.insert("outlet_command".to_string(), "echo outlet".to_string());

        let params = RunnerParams::init(params_map);

        assert_eq!(params.version, "1.5.0");
        assert_eq!(params.state_backend, "s3");
        assert_eq!(params.lock_port, "12345");
        assert_eq!(params.runner_command, Some("terraform".to_string()));
        assert_eq!(params.extra_args, Some("-json".to_string()));
        assert_eq!(params.inlet_command, Some("echo inlet".to_string()));
        assert_eq!(params.outlet_command, Some("echo outlet".to_string()));
    }

    #[test]
    fn test_init_partial_hashmap() {
        let mut params_map: HashMap<String, String> = HashMap::new();
        params_map.insert("version".to_string(), "1.6.0".to_string());
        // Don't set other fields

        let params = RunnerParams::init(params_map);

        assert_eq!(params.version, "1.6.0");
        assert_eq!(params.state_backend, "local"); // serde default
        assert_eq!(params.lock_port, "65432"); // serde default
        assert!(params.runner_command.is_none());
    }

    #[test]
    fn test_get_lock_port() {
        let mut params = create_params_with_defaults();

        // Default value (from serde)
        assert_eq!(params.get_lock_port(), 65432);

        // Custom value
        params.lock_port = "12345".to_string();
        assert_eq!(params.get_lock_port(), 12345);

        // Invalid value should return default
        params.lock_port = "invalid".to_string();
        assert_eq!(params.get_lock_port(), 65432);

        // Empty string should return default
        params.lock_port = "".to_string();
        assert_eq!(params.get_lock_port(), 65432);
    }

    #[test]
    fn test_get_version() {
        let mut params = create_params_with_defaults();
        assert_eq!(params.get_version(), "latest");

        params.version = "1.5.0".to_string();
        assert_eq!(params.get_version(), "1.5.0");
    }

    #[test]
    fn test_get_state_backend() {
        let mut params = create_params_with_defaults();
        assert_eq!(params.get_state_backend(), "local");

        params.state_backend = "s3".to_string();
        assert_eq!(params.get_state_backend(), "s3");
    }

    #[test]
    fn test_get_params_hashmap() {
        let mut params = create_params_with_defaults();
        params.version = "1.5.0".to_string();
        params.runner_command = Some("terraform".to_string());

        let hashmap = params.get_params_hashmap();

        assert_eq!(hashmap.get("version"), Some(&"1.5.0".to_string()));
        assert_eq!(hashmap.get("state_backend"), Some(&"local".to_string()));
        assert_eq!(hashmap.get("lock_port"), Some(&"65432".to_string()));
        assert_eq!(hashmap.get("runner_command"), Some(&"terraform".to_string()));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut params = create_params_with_defaults();
        params.version = "1.6.0".to_string();
        params.state_backend = "s3".to_string();
        params.runner_command = Some("terraform".to_string());
        params.extra_args = Some("-auto-approve".to_string());

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: RunnerParams = serde_json::from_str(&json).unwrap();

        assert_eq!(params.version, deserialized.version);
        assert_eq!(params.state_backend, deserialized.state_backend);
        assert_eq!(params.runner_command, deserialized.runner_command);
        assert_eq!(params.extra_args, deserialized.extra_args);
    }

    #[test]
    fn test_deserialization_with_missing_optional_fields() {
        let json = r#"{"version": "1.5.0", "state_backend": "s3", "lock_port": "65432"}"#;
        let params: RunnerParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.version, "1.5.0");
        assert_eq!(params.state_backend, "s3");
        assert!(params.runner_command.is_none());
        assert!(params.extra_args.is_none());
        assert!(params.inlet_command.is_none());
        assert!(params.outlet_command.is_none());
    }

    #[test]
    fn test_deserialization_with_empty_json() {
        // When deserializing from empty JSON, serde defaults should apply
        let json = "{}";
        let params: RunnerParams = serde_json::from_str(json).unwrap();

        assert_eq!(params.version, "latest");
        assert_eq!(params.state_backend, "local");
        assert_eq!(params.lock_port, "65432");
    }

    #[test]
    fn test_default_functions() {
        assert_eq!(default_version(), "latest");
        assert_eq!(default_state_backend(), "local");
        assert_eq!(default_lock_port(), "65432");
    }

    #[test]
    fn test_clone() {
        let mut params = create_params_with_defaults();
        params.version = "1.5.0".to_string();
        params.runner_command = Some("terraform".to_string());

        let cloned = params.clone();

        assert_eq!(params.version, cloned.version);
        assert_eq!(params.runner_command, cloned.runner_command);
    }
}
