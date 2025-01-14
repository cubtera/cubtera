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
