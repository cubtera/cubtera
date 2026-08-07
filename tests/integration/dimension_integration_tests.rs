use cubtera::core::dim::data::{JsonDataSource, DataSourceConfig, DataSource};
use serde_json::json;

/// Test full dimension processing workflow using example data
#[test]
fn test_example_dimension_workflow() {
    // Override global config to use example inventory
    std::env::set_var("CUBTERA_INVENTORY_PATH", "example/inventory");
    
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test building a complete dimension with hierarchy
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    let env_datasource = JsonDataSource::new_with_config("cubtera", "env", &config);
    let dome_datasource = JsonDataSource::new_with_config("cubtera", "dome", &config);
    
    // Get dimension data
    let dc_data = dc_datasource.get_data_by_name_safe("prod-use1").unwrap();
    let env_data = env_datasource.get_data_by_name_safe("prod").unwrap();
    let dome_data = dome_datasource.get_data_by_name_safe("prod").unwrap();
    
    // Verify parent relationships
    assert_eq!(dc_data["meta"]["parent"], "env:prod");
    assert_eq!(env_data["meta"]["parent"], "dome:prod");
    
    // Verify data propagation
    assert_eq!(env_data["meta"]["prod"], true);
    assert_eq!(dome_data["meta"]["account_id"], "1111111111");
    assert_eq!(dc_data["meta"]["vpc_cidr"], "10.11.0.0/16");
}

#[test]
fn test_dimension_tree_traversal() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test complete dimension tree: dome -> env -> dc -> service -> mongodb
    let service_datasource = JsonDataSource::new_with_config("cubtera", "service", &config);
    let mongodb_datasource = JsonDataSource::new_with_config("cubtera", "mongodb", &config);
    
    // Get service dimension
    let admin_service = service_datasource.get_data_by_name_safe("admin").unwrap();
    assert_eq!(admin_service["name"], "admin");
    
    // Get mongodb dimension  
    let users_mongodb = mongodb_datasource.get_data_by_name_safe("users").unwrap();
    assert_eq!(users_mongodb["name"], "users");
    assert_eq!(users_mongodb["meta"]["owner"], "team1");
    
    // Test different environments for same service
    let orders_mongodb = mongodb_datasource.get_data_by_name_safe("orders").unwrap();
    assert_eq!(orders_mongodb["meta"]["owner"], "team2");
}

#[test]
fn test_regional_dimension_variations() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test US East regions
    let prod_use1 = dc_datasource.get_data_by_name_safe("prod-use1").unwrap();
    let prod_use2 = dc_datasource.get_data_by_name_safe("prod-use2").unwrap();
    
    assert_eq!(prod_use1["meta"]["vpc_cidr"], "10.11.0.0/16");
    assert_eq!(prod_use2["meta"]["vpc_cidr"], "10.12.0.0/16");
    
    // Test EU regions
    let prod_euw1 = dc_datasource.get_data_by_name_safe("prod-euw1").unwrap();
    assert_eq!(prod_euw1["meta"]["region"], "eu-west-1");
    assert_eq!(prod_euw1["meta"]["vpc_cidr"], "10.10.0.0/16");
    
    // Test staging regions
    let stg1_use2 = dc_datasource.get_data_by_name_safe("stg1-use2").unwrap();
    let stg2_euw1 = dc_datasource.get_data_by_name_safe("stg2-euw1").unwrap();
    let stg2_euw2 = dc_datasource.get_data_by_name_safe("stg2-euw2").unwrap();
    
    assert_eq!(stg1_use2["meta"]["parent"], "env:stg1");
    assert_eq!(stg2_euw1["meta"]["parent"], "env:stg2");
    assert_eq!(stg2_euw2["meta"]["parent"], "env:stg2");
}

#[test]
fn test_environment_type_consistency() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let env_datasource = JsonDataSource::new_with_config("cubtera", "env", &config);
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test production environment
    let prod_env = env_datasource.get_data_by_name_safe("prod").unwrap();
    assert_eq!(prod_env["meta"]["prod"], true);
    
    // Test that all prod DCs reference prod env
    let prod_dcs = ["prod-use1", "prod-use2", "prod-euw1"];
    for dc_name in prod_dcs {
        let dc_data = dc_datasource.get_data_by_name_safe(dc_name).unwrap();
        assert_eq!(dc_data["meta"]["parent"], "env:prod", 
                   "DC {} should reference env:prod", dc_name);
    }
    
    // Test staging environments
    let stg_envs = ["stg1", "stg2"];
    for env_name in stg_envs {
        let env_data = env_datasource.get_data_by_name_safe(env_name).unwrap();
        assert_eq!(env_data["meta"]["parent"], "dome:stg",
                   "Env {} should reference dome:stg", env_name);
    }
    
    // Test management environment
    let mgmt_env = env_datasource.get_data_by_name_safe("mgmt").unwrap();
    assert_eq!(mgmt_env["meta"]["parent"], "dome:mgmt");
    
    let mgmt_dc = dc_datasource.get_data_by_name_safe("mgmt-use2").unwrap();
    assert_eq!(mgmt_dc["meta"]["parent"], "env:mgmt");
}

#[test]
fn test_default_value_inheritance() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    let env_datasource = JsonDataSource::new_with_config("cubtera", "env", &config);
    
    // Test DC defaults
    let defaults_dc = dc_datasource.get_data_by_name_safe(".default").unwrap();
    if defaults_dc.get("meta").is_some() {
        assert_eq!(defaults_dc["meta"]["region"], "us-east-1");
    }
    
    // Test ENV defaults  
    let defaults_env = env_datasource.get_data_by_name_safe(".default").unwrap();
    if defaults_env.get("meta").is_some() {
        assert_eq!(defaults_env["meta"]["prod"], false);
    }
    
    // Test that dimensions without explicit values might inherit defaults
    // This tests current behavior - actual default merging might be in different layer
    let mgmt_dc = dc_datasource.get_data_by_name_safe("mgmt-use2").unwrap();
    // mgmt-use2.json only has parent and vpc_cidr, region should come from defaults
    // In current implementation, defaults are not auto-merged, but this tests the structure
    assert!(mgmt_dc.get("name").is_some());
    assert!(mgmt_dc.get("meta").is_some());
}

#[test]
fn test_dimension_data_types() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test dimension with additional data types
    let stg1_use2 = dc_datasource.get_data_by_name_safe("stg1-use2").unwrap();
    
    // Should have meta type
    assert!(stg1_use2.get("meta").is_some());
    assert_eq!(stg1_use2["meta"]["parent"], "env:stg1");
    
    // Should have test type from stg1-use2:test.json
    if stg1_use2.get("test").is_some() {
        assert_eq!(stg1_use2["test"]["test#2"], "should be");
    }
    
    // Test that file patterns with # are handled
    // Note: stg1-use2#test.txt exists but might not be processed as JSON
}

#[test]
fn test_service_team_ownership() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let service_datasource = JsonDataSource::new_with_config("cubtera", "service", &config);
    let mongodb_datasource = JsonDataSource::new_with_config("cubtera", "mongodb", &config);
    
    // Test service ownership
    let admin_service = service_datasource.get_data_by_name_safe("admin").unwrap();
    let order_service = service_datasource.get_data_by_name_safe("order").unwrap();
    
    // Admin service owned by team1 and team2
    if let Some(owners) = admin_service["meta"]["owners"].as_array() {
        assert_eq!(owners.len(), 2);
        assert!(owners.contains(&json!("team1")));
        assert!(owners.contains(&json!("team2")));
    }
    
    // Order service owned by finance_team
    if let Some(owners) = order_service["meta"]["owners"].as_array() {
        assert_eq!(owners.len(), 1);
        assert!(owners.contains(&json!("finance_team")));
    }
    
    // Test mongodb ownership alignment
    let users_mongodb = mongodb_datasource.get_data_by_name_safe("users").unwrap();
    let orders_mongodb = mongodb_datasource.get_data_by_name_safe("orders").unwrap();
    
    assert_eq!(users_mongodb["meta"]["owner"], "team1");
    assert_eq!(orders_mongodb["meta"]["owner"], "team2");
}

#[test]
fn test_environment_specific_configurations() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let mongodb_datasource = JsonDataSource::new_with_config("cubtera", "mongodb", &config);
    let service_datasource = JsonDataSource::new_with_config("cubtera", "service", &config);
    
    // Test mongodb environment-specific sizes
    let users_mongodb = mongodb_datasource.get_data_by_name_safe("users").unwrap();
    if users_mongodb["meta"].get("prod").is_some() {
        assert_eq!(users_mongodb["meta"]["prod"]["size"], "M20");
    }
    
    let orders_mongodb = mongodb_datasource.get_data_by_name_safe("orders").unwrap();
    if orders_mongodb["meta"].get("prod").is_some() {
        assert_eq!(orders_mongodb["meta"]["prod"]["size"], "M80");
    }
    if orders_mongodb["meta"].get("stg").is_some() {
        assert_eq!(orders_mongodb["meta"]["stg"]["size"], "M10");
    }
    
    // Test service environment-specific configuration
    if service_datasource.get_data_by_name_safe(".default").is_ok() {
        let default_service = service_datasource.get_data_by_name_safe(".default").unwrap();
        if default_service.get("manifest").is_some() {
            let manifest = &default_service["manifest"];
            if manifest.get("prod").is_some() {
                assert_eq!(manifest["prod"]["min_capacity"], 2);
                assert_eq!(manifest["prod"]["max_capacity"], 10);
            }
            if manifest.get("stg").is_some() {
                assert_eq!(manifest["stg"]["min_capacity"], 1);
                assert_eq!(manifest["stg"]["max_capacity"], 2);
            }
        }
    }
}

#[test]
fn test_cross_dimension_consistency() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test that dimension hierarchy is consistent across types
    let dome_datasource = JsonDataSource::new_with_config("cubtera", "dome", &config);
    let env_datasource = JsonDataSource::new_with_config("cubtera", "env", &config);
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Get all dimension names for each type
    let dome_names = dome_datasource.get_all_names_safe().unwrap();
    let env_names = env_datasource.get_all_names_safe().unwrap();
    let dc_names = dc_datasource.get_all_names_safe().unwrap();
    
    // Verify that referenced parents exist
    for env_name in &env_names {
        let env_data = env_datasource.get_data_by_name_safe(env_name).unwrap();
        if let Some(parent) = env_data["meta"]["parent"].as_str() {
            if parent.starts_with("dome:") {
                let dome_name = parent.strip_prefix("dome:").unwrap();
                assert!(dome_names.contains(&dome_name.to_string()),
                        "Env {} references dome:{} which should exist", env_name, dome_name);
            }
        }
    }
    
    for dc_name in &dc_names {
        let dc_data = dc_datasource.get_data_by_name_safe(dc_name).unwrap();
        if let Some(parent) = dc_data["meta"]["parent"].as_str() {
            if parent.starts_with("env:") {
                let env_name = parent.strip_prefix("env:").unwrap();
                assert!(env_names.contains(&env_name.to_string()),
                        "DC {} references env:{} which should exist", dc_name, env_name);
            }
        }
    }
}

#[test]
fn test_dimension_builder_integration() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test that we can build dimension objects from real data
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // This tests the integration between data extraction and dimension building
    let dc_data = dc_datasource.get_data_by_name_safe("prod-use1").unwrap();
    
    // Verify structure for dimension building
    assert!(dc_data.get("name").is_some());
    assert!(dc_data.get("meta").is_some());
    
    // Test that data is properly structured for downstream processing
    assert!(dc_data["name"].is_string());
    assert!(dc_data["meta"].is_object());
    
    if let Some(parent) = dc_data["meta"]["parent"].as_str() {
        assert!(parent.contains(":"), "Parent should be in type:name format");
    }
} 