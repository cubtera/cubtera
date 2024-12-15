#![allow(clippy::blocks_in_conditions)]

use crate::prelude::*;
use crate::utils::helper::*;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::Path;
use libc::popen;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bom {
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_name: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // unit_ref_sha: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // unit_blob_sha: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // dims: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // dims_blob_sha: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // dims_ref_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars_blob_sha: Option<String>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // timestamp: Option<u64>,
    // #[serde(skip_serializing_if = "Option::is_none")]
    // datetime: Option<String>,
}

#[allow(clippy::too_many_arguments)]
impl Bom {
    pub fn build(unit: Unit, tf_command: String, exitcode: i32) -> Self {
        // let timestamp = std::time::SystemTime::now()
        //     .duration_since(std::time::UNIX_EPOCH)
        //     .unwrap()
        //     .as_secs();
        // let hr_time = chrono::DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();

        let x = unit.clone().manifest.dimensions;
        //remove first element
        let mut y = std::collections::VecDeque::from(x);
        let z = y.pop_front();
        let a = y
            .iter()
            .map(|x| x.into())
            .collect::<Vec<String>>();
        
        let state_path = unit.get_unit_state_path();
        let dims: HashMap<String, String> = state_path
            .split('/')
            .map(Into::into)
            .map(|dim: String| {
                let parts: Vec<&str> = dim.split(':').collect();
                (parts[0].to_string(), parts[1].to_string())
            })
            .collect();

        let unit_commit_sha = unit.clone().get_unit_commit_sha();

        let inventory_commit_sha = get_commit_sha_by_path(
            &Path::new(&GLOBAL_CFG.inventory_path).to_path_buf()
        ).unwrap_or("undefined".into());


        let unit_blob_sha = unit.clone().get_unit_blob_sha();
        let dims_blob_sha = unit.clone().dimensions
            .iter()
            .map(|dim| (format!("{}:{}", dim.dim_type, dim.dim_name),dim.clone().data_sha))
            .collect::<HashMap<String, String>>();

        let env_vars =  unit.clone().manifest.spec
            .and_then(|spec| spec.env_vars)
            .map(|env_vars| {
                env_vars.optional.iter().flatten()
                    .chain(env_vars.required.iter().flatten())
                    .map(|(_, v)| std::env::var(v).ok().map(|val| (v.clone(), val)))
                    .flatten()
                    .collect::<HashMap<String, String>>()
            });
        
        let env_vars_blob_sha = get_sha_by_value(&json!(env_vars));

        Self {
            unit_name: Some(unit.get_name()),
            // dims: Some(dims),
            // unit_sha: Some(unit_commit_sha),
            // unit_blob_sha: Some(unit_blob_sha),
            // inventory_sha: Some(inventory_commit_sha),
            // dims_blob_sha: Some(dims_blob_sha),
            // timestamp: Some(timestamp),
            // datetime: Some(hr_time.to_string()),
            
            // unit_ref_sha: None,
            // dims_ref_sha: None,
            env_vars,
            env_vars_blob_sha: Some(env_vars_blob_sha),
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
            let col = db.collection::<mongodb::bson::Bson>("bom");
            //let col = MongoCollection::new(org, "dlog", &cl);
            let data = serde_json::json!(self);
            let doc = mongodb::bson::to_bson(&data)?;
            col.insert_one(doc).run()?;
            return Ok(());
        }
        anyhow::bail!("Can't connect to dLog DB");
    }
}

