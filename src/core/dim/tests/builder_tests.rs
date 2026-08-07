use super::super::*;

#[cfg(test)]
mod builder_tests {
    use super::*;

    #[test]
    fn test_builder_basic_creation() {
        // Test basic builder creation without calling methods that need GLOBAL_CFG
        let builder = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({"name": "test_dim"}),
            default_data: serde_json::json!({}),
            datasource: data_src_init("test_org", "test_type", Storage::FS),
            storage: Storage::FS,
        };
        
        assert_eq!(builder.dim_name, "test_dim");
        assert_eq!(builder.dim_type, "test_type");
        assert_eq!(builder.org, "test_org");
        assert_eq!(builder.storage, Storage::FS);
    }

    #[test]
    fn test_builder_data_access() {
        let builder = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({"name": "test_dim", "region": "us-east-1"}),
            default_data: serde_json::json!({"default_key": "default_value"}),
            datasource: data_src_init("test_org", "test_type", Storage::FS),
            storage: Storage::FS,
        };
        
        // Test data access methods
        let data = builder.get_data();
        assert_eq!(data["name"], "test_dim");
        assert_eq!(data["region"], "us-east-1");
        
        let default_data = builder.get_default_data();
        assert_eq!(default_data["default_key"], "default_value");
    }

    #[test]
    fn test_builder_storage_types() {
        let builder_fs = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({}),
            default_data: serde_json::json!({}),
            datasource: data_src_init("test_org", "test_type", Storage::FS),
            storage: Storage::FS,
        };
        
        let builder_db = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({}),
            default_data: serde_json::json!({}),
            datasource: data_src_init("test_org", "test_type", Storage::DB),
            storage: Storage::DB,
        };
        
        assert_eq!(builder_fs.storage, Storage::FS);
        assert_eq!(builder_db.storage, Storage::DB);
    }

    #[test]
    fn test_builder_consistency() {
        let builder1 = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({"name": "test_dim"}),
            default_data: serde_json::json!({}),
            datasource: data_src_init("test_org", "test_type", Storage::FS),
            storage: Storage::FS,
        };
        
        let builder2 = DimBuilder {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            org: "test_org".to_string(),
            dim_path: std::path::PathBuf::from("/test/path"),
            data: serde_json::json!({"name": "test_dim"}),
            default_data: serde_json::json!({}),
            datasource: data_src_init("test_org", "test_type", Storage::FS),
            storage: Storage::FS,
        };
        
        assert_eq!(builder1.dim_name, builder2.dim_name);
        assert_eq!(builder1.dim_type, builder2.dim_type);
        assert_eq!(builder1.org, builder2.org);
        assert_eq!(builder1.storage, builder2.storage);
    }
} 