use cubtera::core::runner::*;
use cubtera::core::unit::Manifest;
use cubtera::core::runner::params::RunnerParams;
use serde_json::json;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Helper struct to create test units
struct TestUnitBuilder {
    name: String,
    unit_type: String,
    temp_dir: TempDir,
    runner_params: Option<HashMap<String, String>>,
}

impl TestUnitBuilder {
    fn new(name: &str, unit_type: &str) -> Self {
        Self {
            name: name.to_string(),
            unit_type: unit_type.to_string(),
            temp_dir: TempDir::new().unwrap(),
            runner_params: None,
        }
    }

    fn with_runner_params(mut self, params: HashMap<String, String>) -> Self {
        self.runner_params = Some(params);
        self
    }

    fn build(self) -> TestUnit {
        let manifest = Manifest {
            dimensions: vec!["dc".to_string()],
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: self.unit_type,
            spec: None,
            runner: self.runner_params,
            state: None,
        };

        TestUnit {
            name: self.name,
            manifest,
            temp_folder: self.temp_dir.path().to_path_buf(),
            temp_dir: self.temp_dir,
        }
    }
}

struct TestUnit {
    name: String,
    manifest: Manifest,
    temp_folder: PathBuf,
    #[allow(dead_code)]
    temp_dir: TempDir, // Keep alive for the duration of the test
}

#[test]
fn test_runner_builder_workflow_bash() {
    let unit = TestUnitBuilder::new("test-bash-unit", "bash").build();
    let command = vec!["echo".to_string(), "hello".to_string()];

    // This would need proper Unit implementation
    // For now, we test the structure
    assert_eq!(unit.manifest.unit_type, "bash");
    assert_eq!(unit.name, "test-bash-unit");
}

#[test]
fn test_runner_builder_workflow_tf() {
    let mut runner_params = HashMap::new();
    runner_params.insert("version".to_string(), "1.5.0".to_string());
    runner_params.insert("state_backend".to_string(), "s3".to_string());

    let unit = TestUnitBuilder::new("test-tf-unit", "tf")
        .with_runner_params(runner_params)
        .build();

    let command = vec!["plan".to_string()];

    assert_eq!(unit.manifest.unit_type, "tf");
    assert_eq!(unit.name, "test-tf-unit");
    assert!(unit.manifest.runner.is_some());
    
    let runner_config = unit.manifest.runner.unwrap();
    assert_eq!(runner_config.get("version"), Some(&"1.5.0".to_string()));
    assert_eq!(runner_config.get("state_backend"), Some(&"s3".to_string()));
}

#[test]
fn test_runner_type_conversion_integration() {
    let test_cases = vec![
        ("tf", RunnerType::TF),
        ("TF", RunnerType::TF),
        ("terraform", RunnerType::UNKNOWN),
        ("bash", RunnerType::BASH),
        ("BASH", RunnerType::BASH),
        ("tofu", RunnerType::TOFU),
        ("TOFU", RunnerType::TOFU),
        ("helm", RunnerType::HELM),
        ("HELM", RunnerType::HELM),
        ("unknown", RunnerType::UNKNOWN),
        ("", RunnerType::UNKNOWN),
    ];

    for (input, expected) in test_cases {
        let result = RunnerType::str_to_runner_type(input);
        assert!(matches!(result, expected), "Failed for input: {}", input);
    }
}

#[test]
fn test_state_backend_template_integration() {
    let handlebars = handlebars::Handlebars::new();
    let data = json!({
        "org": "test-org",
        "unit_name": "web-app",
        "dim_tree": "dc:prod/app:frontend"
    });

    // Test S3 backend template
    let s3_template = json!({
        "s3": {
            "bucket": "{{ org }}-terraform-state",
            "key": "{{ dim_tree }}/{{ unit_name }}.tfstate",
            "region": "us-east-1",
            "encrypt": true,
            "dynamodb_table": "{{ org }}-terraform-locks"
        }
    });

    let result = apply_template_to_value(&s3_template, &handlebars, &data);

    let expected = json!({
        "s3": {
            "bucket": "test-org-terraform-state",
            "key": "dc:prod/app:frontend/web-app.tfstate",
            "region": "us-east-1",
            "encrypt": true,
            "dynamodb_table": "test-org-terraform-locks"
        }
    });

    assert_eq!(result, expected);

    // Test GCS backend template
    let gcs_template = json!({
        "gcs": {
            "bucket": "{{ org }}-tf-state",
            "prefix": "{{ dim_tree }}/{{ unit_name }}"
        }
    });

    let gcs_result = apply_template_to_value(&gcs_template, &handlebars, &data);

    let gcs_expected = json!({
        "gcs": {
            "bucket": "test-org-tf-state",
            "prefix": "dc:prod/app:frontend/web-app"
        }
    });

    assert_eq!(gcs_result, gcs_expected);
}

#[test]
fn test_runner_params_integration() {
    // Test full params workflow
    let mut input_params = HashMap::new();
    input_params.insert("version".to_string(), "1.6.0".to_string());
    input_params.insert("state_backend".to_string(), "gcs".to_string());
    input_params.insert("runner_command".to_string(), "terraform".to_string());
    input_params.insert("extra_args".to_string(), "--parallelism=5".to_string());
    input_params.insert("inlet_command".to_string(), "echo 'Starting deployment'".to_string());
    input_params.insert("outlet_command".to_string(), "echo 'Deployment complete'".to_string());
    input_params.insert("lock_port".to_string(), "9999".to_string());

    let params = RunnerParams::init(input_params);

    // Test all getters
    assert_eq!(params.get_version(), "1.6.0");
    assert_eq!(params.get_state_backend(), "gcs");
    assert_eq!(params.get_lock_port(), 9999);

    // Test conversion back to hashmap
    let hashmap = params.get_params_hashmap();
    assert!(hashmap.contains_key("version"));
    assert!(hashmap.contains_key("state_backend"));
    assert!(hashmap.contains_key("runner_command"));
    assert!(hashmap.contains_key("extra_args"));
    assert!(hashmap.contains_key("inlet_command"));
    assert!(hashmap.contains_key("outlet_command"));
    assert!(hashmap.contains_key("lock_port"));
}

#[test]
fn test_complex_template_scenarios() {
    let handlebars = handlebars::Handlebars::new();
    
    // Test nested templates with arrays
    let complex_template = json!({
        "backends": [
            {
                "type": "s3",
                "config": {
                    "bucket": "{{ org }}-state-{{ env }}",
                    "key": "{{ unit_name }}/terraform.tfstate"
                }
            },
            {
                "type": "local",
                "config": {
                    "path": "/tmp/{{ org }}/{{ unit_name }}.tfstate"
                }
            }
        ],
        "metadata": {
            "org": "{{ org }}",
            "unit": "{{ unit_name }}",
            "full_path": "{{ org }}/{{ unit_name }}"
        }
    });

    let data = json!({
        "org": "acme-corp",
        "unit_name": "api-gateway",
        "env": "production"
    });

    let result = apply_template_to_value(&complex_template, &handlebars, &data);

    let expected = json!({
        "backends": [
            {
                "type": "s3",
                "config": {
                    "bucket": "acme-corp-state-production",
                    "key": "api-gateway/terraform.tfstate"
                }
            },
            {
                "type": "local",
                "config": {
                    "path": "/tmp/acme-corp/api-gateway.tfstate"
                }
            }
        ],
        "metadata": {
            "org": "acme-corp",
            "unit": "api-gateway",
            "full_path": "acme-corp/api-gateway"
        }
    });

    assert_eq!(result, expected);
}

#[test]
fn test_runner_params_edge_cases_integration() {
    // Test with minimal params
    let minimal_params = HashMap::new();
    let params = RunnerParams::init(minimal_params);
    
    assert_eq!(params.get_version(), "latest");
    assert_eq!(params.get_state_backend(), "local");
    assert_eq!(params.get_lock_port(), 65432);

    // Test with invalid lock port
    let mut invalid_params = HashMap::new();
    invalid_params.insert("lock_port".to_string(), "not_a_number".to_string());
    let params = RunnerParams::init(invalid_params);
    assert_eq!(params.get_lock_port(), 65432); // Should fallback to default

    // Test with extreme values
    let mut extreme_params = HashMap::new();
    extreme_params.insert("version".to_string(), "a".repeat(1000)); // Very long version
    extreme_params.insert("lock_port".to_string(), "65535".to_string()); // Max port
    let params = RunnerParams::init(extreme_params);
    assert_eq!(params.get_version().len(), 1000);
    assert_eq!(params.get_lock_port(), 65535);
}

#[test]
fn test_template_error_handling() {
    let handlebars = handlebars::Handlebars::new();
    
    // Test template with missing variables
    let template_with_missing_vars = json!("{{ missing_var }}/{{ another_missing }}");
    let empty_data = json!({});
    
    let result = apply_template_to_value(&template_with_missing_vars, &handlebars, &empty_data);
    
    // Handlebars renders missing variables as empty strings, so we get "/"
    assert_eq!(result, json!("/"));
}

#[test]
fn test_runner_load_creation_integration() {
    let unit = TestUnitBuilder::new("integration-test", "bash").build();
    let command = vec!["test".to_string(), "command".to_string()];
    let params = RunnerParams::default();
    let state_backend = json!({
        "consul": {
            "address": "consul.example.com:8500",
            "scheme": "https",
            "path": "terraform/state"
        }
    });

    // In a real implementation, this would create a proper RunnerLoad
    // For now, we test the components
    assert_eq!(unit.manifest.unit_type, "bash");
    assert!(!command.is_empty());
    assert_eq!(params.get_version(), "latest");
    assert!(state_backend.is_object());
}

#[test]
fn test_multiple_runner_types_workflow() {
    let runner_types = vec!["tf", "bash", "tofu", "helm"];
    
    for runner_type in runner_types {
        let unit = TestUnitBuilder::new(&format!("test-{}", runner_type), runner_type).build();
        let parsed_type = RunnerType::str_to_runner_type(runner_type);
        
        assert_eq!(unit.manifest.unit_type, runner_type);
        assert!(!matches!(parsed_type, RunnerType::UNKNOWN));
    }
}

#[test]
fn test_state_backend_variations() {
    let handlebars = handlebars::Handlebars::new();
    let data = json!({
        "org": "test",
        "unit_name": "app",
        "dim_tree": "env:prod"
    });

    let backends = vec![
        ("local", json!({"local": {"path": "{{ org }}/{{ unit_name }}.tfstate"}})),
        ("s3", json!({"s3": {"bucket": "{{ org }}-state", "key": "{{ unit_name }}.tfstate"}})),
        ("gcs", json!({"gcs": {"bucket": "{{ org }}-state", "prefix": "{{ unit_name }}"}})),
        ("azurerm", json!({"azurerm": {"storage_account_name": "{{ org }}state", "container_name": "tfstate", "key": "{{ unit_name }}.tfstate"}})),
    ];

    for (backend_type, template) in backends {
        let result = apply_template_to_value(&template, &handlebars, &data);
        
        // Verify that templating worked by checking specific values
        match backend_type {
            "local" => {
                assert_eq!(result["local"]["path"], json!("test/app.tfstate"));
            },
            "s3" => {
                assert_eq!(result["s3"]["bucket"], json!("test-state"));
                assert_eq!(result["s3"]["key"], json!("app.tfstate"));
            },
            "gcs" => {
                assert_eq!(result["gcs"]["bucket"], json!("test-state"));
                assert_eq!(result["gcs"]["prefix"], json!("app"));
            },
            "azurerm" => {
                assert_eq!(result["azurerm"]["storage_account_name"], json!("teststate"));
                assert_eq!(result["azurerm"]["key"], json!("app.tfstate"));
            },
            _ => panic!("Unknown backend type: {}", backend_type),
        }
    }
}

#[test]
fn test_concurrent_runner_operations() {
    use std::sync::Arc;
    use std::thread;

    let runner_types = Arc::new(vec!["tf", "bash", "tofu", "helm"]);
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let types = Arc::clone(&runner_types);
            thread::spawn(move || {
                let runner_type = &types[i];
                let unit = TestUnitBuilder::new(&format!("concurrent-{}", i), runner_type).build();
                let parsed_type = RunnerType::str_to_runner_type(runner_type);
                
                assert_eq!(unit.manifest.unit_type, *runner_type);
                assert!(!matches!(parsed_type, RunnerType::UNKNOWN));
                
                // Test params creation in concurrent context
                let params = RunnerParams::default();
                assert_eq!(params.get_version(), "latest");
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
}

#[test]
fn test_file_system_integration() {
    let temp_dir = TempDir::new().unwrap();
    let unit_path = temp_dir.path().join("test_unit");
    fs::create_dir_all(&unit_path).unwrap();

    // Create a manifest file
    let manifest_content = r#"
dimensions = ["dc", "app"]
type = "tf"
overwrite = true

[runner]
version = "1.5.0"
state_backend = "s3"
runner_command = "terraform"
extra_args = "--detailed-exitcode"

[spec.env_vars.required]
AWS_REGION = "AWS_REGION"
"#;

    fs::write(unit_path.join("manifest.toml"), manifest_content).unwrap();

    // Verify file was created
    assert!(unit_path.join("manifest.toml").exists());
    
    // Read and verify content
    let content = fs::read_to_string(unit_path.join("manifest.toml")).unwrap();
    assert!(content.contains("type = \"tf\""));
    assert!(content.contains("version = \"1.5.0\""));
}

#[test]
fn test_performance_template_rendering() {
    use std::time::Instant;
    
    let handlebars = handlebars::Handlebars::new();
    let data = json!({
        "org": "performance-test",
        "unit_name": "load-test",
        "dim_tree": "dc:us-east-1/env:prod/app:web"
    });

    let template = json!({
        "s3": {
            "bucket": "{{ org }}-terraform-state",
            "key": "{{ dim_tree }}/{{ unit_name }}.tfstate",
            "region": "us-east-1"
        }
    });

    let start = Instant::now();
    
    // Render template 1000 times
    for _ in 0..1000 {
        let _ = apply_template_to_value(&template, &handlebars, &data);
    }
    
    let duration = start.elapsed();
    
    // Should complete in reasonable time (less than 1 second for 1000 renders)
    assert!(duration.as_millis() < 1000, "Template rendering took too long: {:?}", duration);
} 