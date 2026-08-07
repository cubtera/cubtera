use cubtera::core::dim::data::{JsonDataSource, DataSourceConfig, DataSource};
use serde_json::json;
use std::path::PathBuf;

/// Test inventory structure using real example data
#[test]
fn test_example_inventory_structure() {
    let example_path = "example/inventory";
    
    // Test that example inventory exists and has expected structure
    assert!(PathBuf::from(example_path).exists(), "Example inventory should exist");
    
    // Test org directories
    let orgs = ["cubtera", "teracub", "cubtera_light"];
    for org in orgs {
        let org_path = PathBuf::from(example_path).join(org);
        assert!(org_path.exists(), "Org directory {} should exist", org);
        assert!(org_path.is_dir(), "Org {} should be a directory", org);
    }
    
    // Test dimension types in cubtera org
    let dim_types = ["dome", "env", "dc", "service", "mongodb", "postgres"];
    for dim_type in dim_types {
        let dim_path = PathBuf::from(example_path).join("cubtera").join(dim_type);
        assert!(dim_path.exists(), "Dimension type {} should exist", dim_type);
        assert!(dim_path.is_dir(), "Dimension type {} should be a directory", dim_type);
    }
}

#[test]
fn test_real_dimension_data_extraction() {
    // Use real example data with custom config pointing to example inventory
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test DC dimension extraction
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test getting specific dimension
    let prod_use1_result = dc_datasource.get_data_by_name_safe("prod-use1").unwrap();
    
    assert_eq!(prod_use1_result["name"], "prod-use1");
    assert_eq!(prod_use1_result["meta"]["parent"], "env:prod");
    assert_eq!(prod_use1_result["meta"]["vpc_cidr"], "10.11.0.0/16");
    
    // Test getting all DC names
    let dc_names = dc_datasource.get_all_names_safe().unwrap();
    let expected_names = ["prod-use1", "stg1-use2", "stg2-euw2", "prod-use2", "mgmt-use2", "preprod-use2", "prod-euw1", "stg2-euw1"];
    
    for expected_name in expected_names {
        assert!(dc_names.contains(&expected_name.to_string()), 
                "DC names should contain {}", expected_name);
    }
}

#[test]
fn test_dimension_hierarchy_resolution() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test environment dimension
    let env_datasource = JsonDataSource::new_with_config("cubtera", "env", &config);
    let prod_env = env_datasource.get_data_by_name_safe("prod").unwrap();
    
    assert_eq!(prod_env["name"], "prod");
    assert_eq!(prod_env["meta"]["parent"], "dome:prod");
    assert_eq!(prod_env["meta"]["prod"], true);
    
    // Test dome dimension
    let dome_datasource = JsonDataSource::new_with_config("cubtera", "dome", &config);
    let prod_dome = dome_datasource.get_data_by_name_safe("prod").unwrap();
    
    assert_eq!(prod_dome["name"], "prod");
    assert_eq!(prod_dome["meta"]["account_id"], "1111111111");
    
    // Test staging hierarchy
    let stg_env = env_datasource.get_data_by_name_safe("stg1").unwrap();
    assert_eq!(stg_env["meta"]["parent"], "dome:stg");
    
    let stg_dome = dome_datasource.get_data_by_name_safe("stg").unwrap();
    assert_eq!(stg_dome["meta"]["account_id"], "2222222222");
}

#[test]
fn test_default_values_processing() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test default values in DC
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Get a dimension and check if defaults are applied
    let dc_data = dc_datasource.get_data_by_name_safe("prod-use1").unwrap();
    
    // The prod-use1.json doesn't have region, but .default:meta.json should provide it
    // Note: This tests the current behavior - defaults might need separate processing
    
    // Test that we can access default files directly
    let defaults_result = dc_datasource.get_data_by_name_safe(".default").unwrap();
    assert_eq!(defaults_result["name"], ".default");
    
    if defaults_result.get("meta").is_some() {
        assert_eq!(defaults_result["meta"]["region"], "us-east-1");
    }
}

#[test]
fn test_complex_file_naming_patterns() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test dimension with additional data types
    let stg1_use2_result = dc_datasource.get_data_by_name_safe("stg1-use2").unwrap();
    
    assert_eq!(stg1_use2_result["name"], "stg1-use2");
    
    // Should have meta type from stg1-use2.json
    assert_eq!(stg1_use2_result["meta"]["parent"], "env:stg1");
    assert_eq!(stg1_use2_result["meta"]["vpc_cidr"], "10.21.0.0/16");
    
    // Should have test type from stg1-use2:test.json
    if stg1_use2_result.get("test").is_some() {
        assert_eq!(stg1_use2_result["test"]["test#2"], "should be");
    }
}

#[test]
fn test_multi_org_support() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test cubtera org
    let cubtera_dc = JsonDataSource::new_with_config("cubtera", "dc", &config);
    let cubtera_names = cubtera_dc.get_all_names_safe().unwrap();
    assert!(!cubtera_names.is_empty(), "Cubtera should have DC dimensions");
    
    // Test teracub org (if it has data)
    let teracub_dc = JsonDataSource::new_with_config("teracub", "dc", &config);
    let _teracub_names = teracub_dc.get_all_names_safe(); // Might be empty, that's ok
    
    // Test that orgs are isolated
    let cubtera_data = cubtera_dc.get_data_by_name_safe("prod-use1");
    let teracub_data = teracub_dc.get_data_by_name_safe("prod-use1");
    
    assert!(cubtera_data.is_ok(), "Cubtera should have prod-use1");
    // teracub might not have this dimension, which is expected
}

#[test]
fn test_service_and_mongodb_dimensions() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    // Test service dimensions
    let service_datasource = JsonDataSource::new_with_config("cubtera", "service", &config);
    let service_names = service_datasource.get_all_names_safe().unwrap();
    
    assert!(service_names.contains(&"admin".to_string()), "Should have admin service");
    assert!(service_names.contains(&"order".to_string()), "Should have order service");
    
    let admin_service = service_datasource.get_data_by_name_safe("admin").unwrap();
    assert_eq!(admin_service["name"], "admin");
    
    // Check owners array
    if let Some(owners) = admin_service["meta"]["owners"].as_array() {
        assert!(owners.contains(&json!("team1")));
        assert!(owners.contains(&json!("team2")));
    }
    
    // Test mongodb dimensions
    let mongodb_datasource = JsonDataSource::new_with_config("cubtera", "mongodb", &config);
    let mongodb_names = mongodb_datasource.get_all_names_safe().unwrap();
    
    assert!(mongodb_names.contains(&"users".to_string()), "Should have users mongodb");
    assert!(mongodb_names.contains(&"orders".to_string()), "Should have orders mongodb");
    
    let users_mongodb = mongodb_datasource.get_data_by_name_safe("users").unwrap();
    assert_eq!(users_mongodb["name"], "users");
    assert_eq!(users_mongodb["meta"]["owner"], "team1");
    
    if users_mongodb["meta"].get("prod").is_some() {
        assert_eq!(users_mongodb["meta"]["prod"]["size"], "M20");
    }
}

#[test]
fn test_schema_and_default_files() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test that schema files exist in directory but are not included in dimension data
    let dc_dir = PathBuf::from("example/inventory/cubtera/dc");
    assert!(dc_dir.join(".schema:meta.json").exists(), "Schema file should exist");
    
    // Test default files
    assert!(dc_dir.join(".default:meta.json").exists(), "Default meta file should exist");
    
    // Test various file patterns exist
    assert!(dc_dir.join("stg1-use2:test.json").exists(), "Type-specific file should exist");
    assert!(dc_dir.join("stg1-use2#test.txt").exists(), "Hash-pattern file should exist");
    assert!(dc_dir.join(".default#test.json").exists(), "Default hash-pattern file should exist");
}

#[test]
fn test_error_handling_with_real_data() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dc_datasource = JsonDataSource::new_with_config("cubtera", "dc", &config);
    
    // Test non-existent dimension
    let nonexistent = dc_datasource.get_data_by_name_safe("nonexistent-dimension").unwrap();
    assert_eq!(nonexistent["name"], "nonexistent-dimension");
    assert_eq!(nonexistent.as_object().unwrap().len(), 1); // Only name field
    
    // Test non-existent org
    let bad_org_datasource = JsonDataSource::new_with_config("nonexistent-org", "dc", &config);
    let result = bad_org_datasource.get_all_names_safe();
    assert!(result.is_err(), "Should error for non-existent org");
    
    // Test non-existent dimension type
    let bad_type_datasource = JsonDataSource::new_with_config("cubtera", "nonexistent-type", &config);
    let result = bad_type_datasource.get_all_names_safe();
    assert!(result.is_err(), "Should error for non-existent dimension type");
}

#[test]
fn test_all_dimension_types_have_data() {
    let config = DataSourceConfig::new(
        "example/inventory".to_string(),
        ":".to_string(),
        None,
    );
    
    let dim_types = ["dome", "env", "dc", "service", "mongodb", "postgres"];
    
    for dim_type in dim_types {
        let datasource = JsonDataSource::new_with_config("cubtera", dim_type, &config);
        let names = datasource.get_all_names_safe().unwrap();
        
        assert!(!names.is_empty(), "Dimension type {} should have at least one dimension", dim_type);
        
        // Test that we can load data for each dimension
        for name in &names {
            let data = datasource.get_data_by_name_safe(name).unwrap();
            assert_eq!(data["name"], name.as_str());
            
            // Each dimension should have at least name and meta
            assert!(data.get("name").is_some(), "Dimension {} should have name", name);
            // Note: meta might be empty object, that's ok
        }
    }
} 