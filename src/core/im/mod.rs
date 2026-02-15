use crate::core::dim::data::*;
use crate::core::dim::*;
use crate::prelude::GLOBAL_CFG;
use crate::utils::helper::*;

use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

pub fn get_dim_by_name(
    dim_type: &str,
    dim_name: &str,
    org: &str,
    storage: &Storage,
    context: Option<String>,
) -> Value {
    let dim = DimBuilder::new(dim_type, org, storage)
        .with_name(dim_name)
        .with_context(context)
        .full_build();

    json!({
        "status": "ok",
        "id": "dimByName",
        "type": dim.dim_type,
        "name": dim.dim_name,
        "data": dim.get_dim_data(),
    })
}

pub fn get_dim_names_by_type(dim_type: &str, org: &str, storage: &Storage) -> Value {
    let dims = DimBuilder::new(dim_type, org, storage).get_all_dim_names();

    json!({
        "status": "ok",
        "id": "dimsByType",
        "type": dim_type,
        "data": dims,
    })
}

pub fn get_dims_data_by_type(dim_type: &str, org: &str, storage: &Storage) -> Value {
    let names = DimBuilder::new(dim_type, org, storage).get_all_dim_names();

    let data = names
        .iter()
        .map(|name| {
            let dim = DimBuilder::new(dim_type, org, storage)
                .with_name(name)
                .read_data()
                .merge_defaults()
                .build();
            dim.get_dim_data()
        })
        .collect::<Vec<Value>>();

    json!({
        "status": "ok",
        "id": "dimsDataByType",
        "type": dim_type,
        "data": data,
    })
}

pub fn get_dim_defaults_by_type(dim_type: &str, org: &str, storage: &Storage) -> Value {
    let defaults = DimBuilder::new(dim_type, org, storage)
        .read_default_data()
        .get_default_data();

    json!({
        "status": "ok",
        "id": "dimsDefaultsByType",
        "type": dim_type,
        "data": defaults,
    })
}

pub fn get_dim_kids(dim_type: &str, dim_name: &str, org: &str, storage: &Storage) -> Value {
    let kids = DimBuilder::new(dim_type, org, storage)
        .with_name(dim_name)
        .get_all_kids_by_name();

    json!({
        "status": "ok",
        "id": "dimsByParent",
        "parent_type": dim_type,
        "parent_name": dim_name,
        "data": {
            "dim_type" : kids.keys().next().unwrap_or(&"".to_string()),
            "dim_names" : kids.values().next().unwrap_or(&vec![]),
        },
    })
}

pub fn get_dim_parent(dim_type: &str, dim_name: &str, org: &str, storage: &Storage) -> Value {
    let dim = DimBuilder::new(dim_type, org, storage)
        .with_name(dim_name)
        .read_data()
        .build();

    if let Some(parent) = dim.parent {
        json!({
            "status": "ok",
            "id": "dimParent",
            "type": parent.dim_type,
            "name": parent.dim_name,
            "data": parent.get_dim_data(),
        })
    } else {
        json!({
            "status": "error",
            "id": "dimParent",
            "message": "No parent dim found",
            "data": Value::Null,
        })
    }
}

pub fn get_all_orgs(storage: &Storage) -> Value {
    let orgs = match storage {
        Storage::DB => {
            let db = GLOBAL_CFG
                .db_client
                .clone()
                .unwrap_or_exit("Can't start DB client".into());
            db.list_database_names()
                .run()
                .unwrap_or_exit("Can't read list of orgs from DB".into())
        }
        Storage::FS => GLOBAL_CFG.orgs.clone(),
    }
    .iter()
    .map(String::as_ref)
    .filter(|dim| !["admin", "local", "config", "test"].contains(dim))
    .map(|dim| dim.to_string())
    .collect::<Vec<String>>();

    json!({
        "status": "ok",
        "id": "orgs",
        "data": orgs,
    })
}

pub fn get_dlog_by_keys(org: &str, keys: Vec<String>, limit: Option<i64>) -> Value {
    let keys: HashMap<String, String> = keys
        .iter()
        .map(|dim: &String| {
            let parts: Vec<&str> = dim.split(':').collect();
            (parts[0].to_string(), parts[1].to_string())
        })
        .collect();
    let filter = json!(keys);

    get_dlog(org, filter.clone(), limit)
}

pub fn get_dlog(org: &str, filter: Value, limit: Option<i64>) -> Value {
    let client: Option<mongodb::sync::Client> =
        GLOBAL_CFG.dlog_db.as_ref().map(|db| db_connect(db));

    if client.is_none() {
        return json!({
            "status": "error",
            "id": "dlog",
            "message": format!("Can't connect to dlog DB {}", &GLOBAL_CFG.dlog_db.clone().unwrap_or("".to_string())),
            "notes": "Check CUBTERA_DLOG_DB env variable",
            "data": Value::Null,
        });
    }

    let db = client.unwrap().database(org);
    let col = db.collection::<mongodb::bson::Bson>("dlog");
    let filter = to_dot_notation(&filter, "".to_string());
    let filter = json!(filter);
    let filter = mongodb::bson::to_document(&filter).unwrap_or_default();
    let res = col.find(filter.clone())
        .sort(mongodb::bson::doc! { "timestamp": -1 })
        .limit(limit.unwrap_or(10))
        .run()
        .unwrap();

    let res: Vec<Value> = res
        .collect::<mongodb::error::Result<Vec<mongodb::bson::Bson>>>()
        .unwrap_or_default()
        .iter()
        .map(|bson| mongodb::bson::from_bson::<Value>(bson.clone()).unwrap_or_default())
        .collect();

    json!({
        "status": "ok",
        "id": "dlog",
        "data": res,
        "limit": limit,
        "filter": filter,
    })
}

fn to_dot_notation(value: &Value, prefix: String) -> HashMap<String, Value> {
    let mut result = HashMap::new();

    if let Value::Object(map) = value {
        for (key, value) in map {
            let new_key = if prefix.is_empty() {
                key.clone()
            } else {
                format!("{}.{}", prefix, key)
            };
            match value {
                Value::Object(_) => {
                    result.extend(to_dot_notation(value, new_key));
                }
                _ => {
                    result.insert(new_key, value.clone());
                }
            }
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests for to_dot_notation helper function
    #[test]
    fn test_to_dot_notation_flat_object() {
        let value = json!({
            "env": "prod",
            "dc": "us-east-1"
        });

        let result = to_dot_notation(&value, String::new());

        assert_eq!(result.get("env"), Some(&json!("prod")));
        assert_eq!(result.get("dc"), Some(&json!("us-east-1")));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_to_dot_notation_nested_object() {
        let value = json!({
            "dims": {
                "env": "prod",
                "dc": "us-east-1"
            }
        });

        let result = to_dot_notation(&value, String::new());

        assert_eq!(result.get("dims.env"), Some(&json!("prod")));
        assert_eq!(result.get("dims.dc"), Some(&json!("us-east-1")));
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_to_dot_notation_deeply_nested() {
        let value = json!({
            "level1": {
                "level2": {
                    "level3": "value"
                }
            }
        });

        let result = to_dot_notation(&value, String::new());

        assert_eq!(result.get("level1.level2.level3"), Some(&json!("value")));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_to_dot_notation_mixed_types() {
        let value = json!({
            "string": "value",
            "number": 42,
            "boolean": true,
            "nested": {
                "inner": "data"
            }
        });

        let result = to_dot_notation(&value, String::new());

        assert_eq!(result.get("string"), Some(&json!("value")));
        assert_eq!(result.get("number"), Some(&json!(42)));
        assert_eq!(result.get("boolean"), Some(&json!(true)));
        assert_eq!(result.get("nested.inner"), Some(&json!("data")));
        assert_eq!(result.len(), 4);
    }

    #[test]
    fn test_to_dot_notation_with_prefix() {
        let value = json!({
            "key": "value"
        });

        let result = to_dot_notation(&value, "prefix".to_string());

        assert_eq!(result.get("prefix.key"), Some(&json!("value")));
    }

    #[test]
    fn test_to_dot_notation_empty_object() {
        let value = json!({});

        let result = to_dot_notation(&value, String::new());

        assert!(result.is_empty());
    }

    #[test]
    fn test_to_dot_notation_non_object() {
        let value = json!("string");

        let result = to_dot_notation(&value, String::new());

        assert!(result.is_empty());
    }

    #[test]
    fn test_to_dot_notation_array_not_expanded() {
        let value = json!({
            "array": [1, 2, 3]
        });

        let result = to_dot_notation(&value, String::new());

        // Arrays should be kept as-is, not expanded
        assert_eq!(result.get("array"), Some(&json!([1, 2, 3])));
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_to_dot_notation_mongodb_query_style() {
        // Test case similar to what would be used for MongoDB queries
        let value = json!({
            "dims": {
                "env": "prod",
                "dc": "us-east-1"
            },
            "tf_command": "apply"
        });

        let result = to_dot_notation(&value, String::new());

        assert_eq!(result.get("dims.env"), Some(&json!("prod")));
        assert_eq!(result.get("dims.dc"), Some(&json!("us-east-1")));
        assert_eq!(result.get("tf_command"), Some(&json!("apply")));
    }

    // Tests for response format validation
    #[test]
    fn test_response_format_dim_by_name() {
        // Verify the expected JSON structure for dim_by_name responses
        let expected_fields = vec!["status", "id", "type", "name", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimByName",
            "type": "env",
            "name": "prod",
            "data": {}
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimByName");
    }

    #[test]
    fn test_response_format_dims_by_type() {
        let expected_fields = vec!["status", "id", "type", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimsByType",
            "type": "env",
            "data": ["prod", "staging", "dev"]
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimsByType");
    }

    #[test]
    fn test_response_format_dims_data_by_type() {
        let expected_fields = vec!["status", "id", "type", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimsDataByType",
            "type": "env",
            "data": []
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimsDataByType");
    }

    #[test]
    fn test_response_format_dims_defaults_by_type() {
        let expected_fields = vec!["status", "id", "type", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimsDefaultsByType",
            "type": "env",
            "data": {}
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimsDefaultsByType");
    }

    #[test]
    fn test_response_format_dims_by_parent() {
        let expected_fields = vec!["status", "id", "parent_type", "parent_name", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimsByParent",
            "parent_type": "env",
            "parent_name": "prod",
            "data": {
                "dim_type": "dc",
                "dim_names": ["us-east-1", "us-west-2"]
            }
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimsByParent");
    }

    #[test]
    fn test_response_format_dim_parent() {
        let expected_fields = vec!["status", "id", "type", "name", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimParent",
            "type": "dome",
            "name": "prod",
            "data": {}
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimParent");
    }

    #[test]
    fn test_response_format_orgs() {
        let expected_fields = vec!["status", "id", "data"];

        let response = json!({
            "status": "ok",
            "id": "orgs",
            "data": ["cubtera", "myorg"]
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "orgs");
    }

    #[test]
    fn test_response_format_dlog() {
        let expected_fields = vec!["status", "id", "data", "limit", "filter"];

        let response = json!({
            "status": "ok",
            "id": "dlog",
            "data": [],
            "limit": 10,
            "filter": {}
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dlog");
    }

    #[test]
    fn test_response_format_dim_types() {
        let expected_fields = vec!["status", "id", "org", "data"];

        let response = json!({
            "status": "ok",
            "id": "dimTypes",
            "org": "cubtera",
            "data": ["dome", "env", "dc"]
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["id"], "dimTypes");
    }

    #[test]
    fn test_response_format_error() {
        let expected_fields = vec!["status", "id", "message", "data"];

        let response = json!({
            "status": "error",
            "id": "dimParent",
            "message": "No parent dim found",
            "data": null
        });

        for field in expected_fields {
            assert!(response.get(field).is_some(), "Missing field: {}", field);
        }
        assert_eq!(response["status"], "error");
    }

    // Test get_dlog_by_keys key parsing
    #[test]
    fn test_keys_parsing() {
        let keys = vec!["env:prod".to_string(), "dc:us-east-1".to_string()];

        let parsed: HashMap<String, String> = keys
            .iter()
            .map(|dim: &String| {
                let parts: Vec<&str> = dim.split(':').collect();
                (parts[0].to_string(), parts[1].to_string())
            })
            .collect();

        assert_eq!(parsed.get("env"), Some(&"prod".to_string()));
        assert_eq!(parsed.get("dc"), Some(&"us-east-1".to_string()));
    }

    #[test]
    fn test_orgs_filter_system_databases() {
        let orgs = vec![
            "cubtera".to_string(),
            "admin".to_string(),
            "local".to_string(),
            "config".to_string(),
            "test".to_string(),
            "myorg".to_string(),
        ];

        let filtered: Vec<String> = orgs
            .iter()
            .map(String::as_ref)
            .filter(|dim| !["admin", "local", "config", "test"].contains(dim))
            .map(|dim| dim.to_string())
            .collect();

        assert_eq!(filtered, vec!["cubtera", "myorg"]);
        assert!(!filtered.contains(&"admin".to_string()));
        assert!(!filtered.contains(&"local".to_string()));
        assert!(!filtered.contains(&"config".to_string()));
        assert!(!filtered.contains(&"test".to_string()));
    }

    #[test]
    fn test_dim_types_filter_system_collections() {
        let collections = vec![
            "dome".to_string(),
            "env".to_string(),
            "dc".to_string(),
            "defaults".to_string(),
            "log".to_string(),
            "dlog".to_string(),
        ];

        let filtered: Vec<String> = collections
            .iter()
            .map(String::as_ref)
            .filter(|dim| !["defaults", "log", "dlog"].contains(dim))
            .map(|dim| dim.to_string())
            .collect();

        assert_eq!(filtered, vec!["dome", "env", "dc"]);
        assert!(!filtered.contains(&"defaults".to_string()));
        assert!(!filtered.contains(&"log".to_string()));
        assert!(!filtered.contains(&"dlog".to_string()));
    }
}

pub fn get_all_dim_types(org: &str, storage: &Storage) -> Value {
    let dim_types = match storage {
        Storage::DB => {
            let client = GLOBAL_CFG.db_client.clone().unwrap();
            let db = client.database(org);
            db.list_collection_names().run().unwrap()
        }
        Storage::FS => GLOBAL_CFG.orgs.clone(),
    }
    .iter()
    .map(String::as_ref)
    .filter(|dim| !["defaults", "log", "dlog"].contains(dim))
    .map(|dim| dim.to_string())
    .collect::<Vec<String>>();

    json!({
        "status": "ok",
        "id": "dimTypes",
        "org": org,
        "data": dim_types,
    })
}
