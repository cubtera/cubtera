#![allow(clippy::blocks_in_conditions)]

use crate::prelude::*;
use crate::utils::helper::*;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dlog {
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    state_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dims: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_host_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_user_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_number: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    tf_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    exitcode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_blob_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inventory_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dims_blob_sha: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    datetime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extended_log: Option<HashMap<String, String>>,
}

#[allow(clippy::too_many_arguments)]
impl Dlog {
    pub fn build(unit: Unit, tf_command: String, exitcode: i32) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hr_time = chrono::DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();
        let state_path = unit.get_unit_state_path();
        let dims: HashMap<String, String> = state_path
            .split('/')
            .map(Into::into)
            .map(|dim: String| {
                let parts: Vec<&str> = dim.split(':').collect();
                (parts[0].to_string(), parts[1].to_string())
            })
            .collect();

        // try to get names of env vars from config or use default values
        // useful for CI/CD system job (Jenkins, Gitlab CI, etc.)
        let job_user_name = GLOBAL_CFG
            .dlog_job_user_name_env
            .clone()
            .and_then(|var| std::env::var(var).ok())
            .or(whoami::username().into())
            .unwrap_or("undefined".into());
        let job_build_number = GLOBAL_CFG
            .dlog_job_number_env
            .clone()
            .and_then(|var| std::env::var(var).ok())
            .unwrap_or("0".to_string());
        let job_name = GLOBAL_CFG
            .dlog_job_name_env
            .clone()
            .and_then(|var| std::env::var(var).ok())
            .unwrap_or("undefined".into());

        let inventory_commit_sha = get_commit_sha_by_path(
            &Path::new(&GLOBAL_CFG.inventory_path).to_path_buf()
        ).unwrap_or("undefined".into());
        let unit_commit_sha = unit.get_unit_commit_sha();
        let unit_blob_sha = unit.get_unit_blob_sha();
        let dims_blob_sha = unit.get_dims_blob_sha();
        let env_vars =  unit.get_env_vars();

        Self {
            unit_name: Some(unit.get_name()),
            state_path: Some(state_path),
            dims: Some(dims),
            job_host_name: whoami::fallible::hostname().ok(),
            job_user_name: Some(job_user_name),
            job_number: Some(job_build_number),
            job_name: Some(job_name),
            tf_command: Some(tf_command),
            exitcode: Some(exitcode),
            unit_sha: Some(unit_commit_sha),
            unit_blob_sha: Some(unit_blob_sha),
            inventory_sha: Some(inventory_commit_sha),
            dims_blob_sha: Some(dims_blob_sha),
            timestamp: Some(timestamp),
            datetime: Some(hr_time.to_string()),
            extended_log: get_extended_log(),
            env_vars,
        }
    }

    /// Inserts a log entry into the MongoDB collection for the specified organization.
    ///
    /// # Arguments
    ///
    /// * `org` - The name of the organization to insert the log entry for.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the log entry was successfully inserted, otherwise returns an `anyhow::Error`.
    pub fn put(&self, org: &str) -> anyhow::Result<()> {
        let client: Option<mongodb::sync::Client> =
            GLOBAL_CFG.dlog_db.as_ref().map(|db| db_connect(db));
        if let Some(cl) = client {
            let db = cl.database(org);
            let col = db.collection::<mongodb::bson::Bson>("dlog");
            //let col = MongoCollection::new(org, "dlog", &cl);
            let data = serde_json::json!(self);
            let doc = mongodb::bson::to_bson(&data)?;
            col.insert_one(doc).run()?;
            return Ok(());
        }
        anyhow::bail!("Can't connect to dLog DB");
    }
}

/// Reads extended log data from standard input and returns it as a JSON value.
///
/// # Returns
///
/// An `Option` containing the JSON value if the standard input is not a terminal, otherwise `None`.
fn get_extended_log() -> Option<HashMap<String, String>> {
    use std::io::Read;
    if unsafe { libc::isatty(libc::STDIN_FILENO) == 0 } {
        let mut buffer = String::new();
        std::io::stdin()
            .read_to_string(&mut buffer)
            .unwrap_or_default();
        let buffer = buffer.trim().replace('\n', " ");
        let log_data: Value = serde_json::from_str(&buffer)
            .check_with_warn("Skip extended log data. Broken JSON")
            .unwrap_or_default();
        let extended_log: Option<HashMap<String, String>> = serde_json::from_value(log_data).ok();
        return extended_log;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // Helper to create a minimal Dlog for testing
    fn create_test_dlog() -> Dlog {
        let mut dims: HashMap<String, String> = HashMap::new();
        dims.insert("env".to_string(), "prod".to_string());
        dims.insert("dc".to_string(), "us-east-1".to_string());

        let mut dims_blob_sha: HashMap<String, String> = HashMap::new();
        dims_blob_sha.insert("env:prod".to_string(), "sha1".to_string());
        dims_blob_sha.insert("dc:us-east-1".to_string(), "sha2".to_string());

        Dlog {
            unit_name: Some("network".to_string()),
            state_path: Some("env:prod/dc:us-east-1".to_string()),
            dims: Some(dims),
            job_host_name: Some("localhost".to_string()),
            job_user_name: Some("testuser".to_string()),
            job_number: Some("123".to_string()),
            job_name: Some("deploy-network".to_string()),
            tf_command: Some("apply".to_string()),
            exitcode: Some(0),
            unit_sha: Some("abc123".to_string()),
            unit_blob_sha: Some("def456".to_string()),
            inventory_sha: Some("ghi789".to_string()),
            dims_blob_sha: Some(dims_blob_sha),
            env_vars: None,
            timestamp: Some(1704067200), // 2024-01-01 00:00:00 UTC
            datetime: Some("2024-01-01 00:00:00 UTC".to_string()),
            extended_log: None,
        }
    }

    #[test]
    fn test_dlog_serialization() {
        let dlog = create_test_dlog();

        let json = serde_json::to_string(&dlog).unwrap();

        assert!(json.contains("network"));
        assert!(json.contains("env:prod/dc:us-east-1"));
        assert!(json.contains("apply"));
        assert!(json.contains("testuser"));
    }

    #[test]
    fn test_dlog_deserialization() {
        let json = r#"{
            "unit_name": "network",
            "state_path": "env:prod/dc:us-east-1",
            "tf_command": "apply",
            "exitcode": 0,
            "timestamp": 1704067200
        }"#;

        let dlog: Dlog = serde_json::from_str(json).unwrap();

        assert_eq!(dlog.unit_name, Some("network".to_string()));
        assert_eq!(dlog.state_path, Some("env:prod/dc:us-east-1".to_string()));
        assert_eq!(dlog.tf_command, Some("apply".to_string()));
        assert_eq!(dlog.exitcode, Some(0));
        assert_eq!(dlog.timestamp, Some(1704067200));
    }

    #[test]
    fn test_dlog_serialization_skips_none_fields() {
        let dlog = Dlog {
            unit_name: Some("test".to_string()),
            state_path: None,
            dims: None,
            job_host_name: None,
            job_user_name: None,
            job_number: None,
            job_name: None,
            tf_command: None,
            exitcode: None,
            unit_sha: None,
            unit_blob_sha: None,
            inventory_sha: None,
            dims_blob_sha: None,
            env_vars: None,
            timestamp: None,
            datetime: None,
            extended_log: None,
        };

        let json = serde_json::to_string(&dlog).unwrap();

        // None fields should not be in the JSON output
        assert!(!json.contains("state_path"));
        assert!(!json.contains("job_host_name"));
        assert!(!json.contains("tf_command"));
        assert!(json.contains("unit_name"));
    }

    #[test]
    fn test_dlog_clone() {
        let dlog = create_test_dlog();
        let cloned = dlog.clone();

        assert_eq!(dlog.unit_name, cloned.unit_name);
        assert_eq!(dlog.state_path, cloned.state_path);
        assert_eq!(dlog.exitcode, cloned.exitcode);
        assert_eq!(dlog.tf_command, cloned.tf_command);
    }

    #[test]
    fn test_dlog_debug() {
        let dlog = create_test_dlog();
        let debug_str = format!("{:?}", dlog);

        assert!(debug_str.contains("Dlog"));
        assert!(debug_str.contains("network"));
    }

    #[test]
    fn test_dlog_roundtrip() {
        let original = create_test_dlog();

        let json = serde_json::to_string(&original).unwrap();
        let restored: Dlog = serde_json::from_str(&json).unwrap();

        assert_eq!(original.unit_name, restored.unit_name);
        assert_eq!(original.state_path, restored.state_path);
        assert_eq!(original.dims, restored.dims);
        assert_eq!(original.job_host_name, restored.job_host_name);
        assert_eq!(original.job_user_name, restored.job_user_name);
        assert_eq!(original.job_number, restored.job_number);
        assert_eq!(original.job_name, restored.job_name);
        assert_eq!(original.tf_command, restored.tf_command);
        assert_eq!(original.exitcode, restored.exitcode);
        assert_eq!(original.timestamp, restored.timestamp);
    }

    #[test]
    fn test_dlog_with_env_vars() {
        let mut env_vars: HashMap<String, String> = HashMap::new();
        env_vars.insert("AWS_REGION".to_string(), "us-east-1".to_string());
        env_vars.insert("TF_VAR_env".to_string(), "prod".to_string());

        let mut dlog = create_test_dlog();
        dlog.env_vars = Some(env_vars);

        let json = serde_json::to_string(&dlog).unwrap();

        assert!(json.contains("AWS_REGION"));
        assert!(json.contains("us-east-1"));
        assert!(json.contains("TF_VAR_env"));
    }

    #[test]
    fn test_dlog_with_extended_log() {
        let mut extended_log: HashMap<String, String> = HashMap::new();
        extended_log.insert("plan_output".to_string(), "No changes".to_string());
        extended_log.insert("duration".to_string(), "120s".to_string());

        let mut dlog = create_test_dlog();
        dlog.extended_log = Some(extended_log);

        let json = serde_json::to_string(&dlog).unwrap();

        assert!(json.contains("plan_output"));
        assert!(json.contains("No changes"));
        assert!(json.contains("duration"));
    }

    #[test]
    fn test_dlog_exitcode_values() {
        let mut dlog = create_test_dlog();

        // Success
        dlog.exitcode = Some(0);
        let json = serde_json::to_string(&dlog).unwrap();
        assert!(json.contains("\"exitcode\":0"));

        // Failure
        dlog.exitcode = Some(1);
        let json = serde_json::to_string(&dlog).unwrap();
        assert!(json.contains("\"exitcode\":1"));

        // Other exit codes
        dlog.exitcode = Some(2);
        let json = serde_json::to_string(&dlog).unwrap();
        assert!(json.contains("\"exitcode\":2"));
    }

    #[test]
    fn test_dlog_tf_commands() {
        let commands = vec!["plan", "apply", "destroy", "init", "refresh"];

        for cmd in commands {
            let mut dlog = create_test_dlog();
            dlog.tf_command = Some(cmd.to_string());

            let json = serde_json::to_string(&dlog).unwrap();
            assert!(json.contains(cmd));
        }
    }

    #[test]
    fn test_dlog_dims_parsing() {
        let dlog = create_test_dlog();

        let dims = dlog.dims.unwrap();
        assert_eq!(dims.get("env"), Some(&"prod".to_string()));
        assert_eq!(dims.get("dc"), Some(&"us-east-1".to_string()));
    }

    #[test]
    fn test_dlog_to_json_value() {
        let dlog = create_test_dlog();

        let value: Value = serde_json::to_value(&dlog).unwrap();

        assert!(value.is_object());
        assert_eq!(value["unit_name"], "network");
        assert_eq!(value["tf_command"], "apply");
        assert_eq!(value["exitcode"], 0);
    }

    #[test]
    fn test_dims_extraction_from_state_path() {
        // Simulating what Dlog::build does
        let state_path = "dome:prod/env:production/dc:us-east-1";
        let dims: HashMap<String, String> = state_path
            .split('/')
            .map(Into::into)
            .map(|dim: String| {
                let parts: Vec<&str> = dim.split(':').collect();
                (parts[0].to_string(), parts[1].to_string())
            })
            .collect();

        assert_eq!(dims.len(), 3);
        assert_eq!(dims.get("dome"), Some(&"prod".to_string()));
        assert_eq!(dims.get("env"), Some(&"production".to_string()));
        assert_eq!(dims.get("dc"), Some(&"us-east-1".to_string()));
    }

    #[test]
    fn test_dlog_deserialization_from_bson_compatible_json() {
        // Test that Dlog can deserialize from JSON that might come from MongoDB
        let json = json!({
            "unit_name": "test-unit",
            "state_path": "env:staging/dc:eu-west-1",
            "dims": {
                "env": "staging",
                "dc": "eu-west-1"
            },
            "tf_command": "plan",
            "exitcode": 0,
            "timestamp": 1704067200,
            "datetime": "2024-01-01T00:00:00Z"
        });

        let dlog: Dlog = serde_json::from_value(json).unwrap();

        assert_eq!(dlog.unit_name, Some("test-unit".to_string()));
        assert_eq!(dlog.tf_command, Some("plan".to_string()));
        assert_eq!(dlog.dims.as_ref().unwrap().get("env"), Some(&"staging".to_string()));
    }
}
