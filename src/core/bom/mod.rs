#![allow(clippy::blocks_in_conditions)]

use crate::prelude::*;
use crate::utils::helper::*;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use serde_json::json;
use sha2::{Sha256, Digest};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bom {
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    unit_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    unit_blob_sha: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    inventory_sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inventory_blob_sha: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env_vars_blob_sha: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    bom_atom_blob_sha: Option<String>, // composite sha of unit, inventory, env_vars
    #[serde(skip_serializing_if = "Option::is_none")]
    bom_atom_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    bom_dim_id: Option<String>,

    // _id: Option<String>,
    //

    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    datetime: Option<String>,
}

#[allow(clippy::too_many_arguments)]
impl Bom {
    pub fn build(unit: Unit) -> Self {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let hr_time = chrono::DateTime::from_timestamp(timestamp as i64, 0).unwrap_or_default();

        let unit_commit_sha = unit.get_unit_commit_sha();
        let unit_blob_sha = unit.get_unit_blob_sha();

        let inventory_sha = get_commit_sha_by_path(
            &Path::new(&GLOBAL_CFG.inventory_path).to_path_buf()
        ).unwrap_or("undefined".into());

        let inventory_blob_sha = unit.get_dims_blob_sha();
        let env_vars =  unit.get_env_vars();

        let mut y_dims = unit.dimensions.clone();
        let x_dim = y_dims.remove(0);
        dbg!(x_dim.data_sha);

        let y_hashes = y_dims.iter()
            .map(|dim| dim.data_sha.clone())
            .fold(Sha256::new(), |mut acc, sha| {
                acc.update(sha.as_bytes());
                acc
            }).finalize();

        let inventory_blob_sha = format!("{:x}", y_hashes);
        let env_vars_blob_sha = get_sha_by_value(&json!(env_vars));

        let bom_atom_blob_sha = format!("{:}", get_sha_by_value(&json!(
            vec![
                unit_blob_sha.clone(),
                inventory_blob_sha.clone(),
                env_vars_blob_sha.clone()
            ]
        )));


        let bom_atom_id = y_dims.is_empty()
            .then_some(unit.get_name())
            .unwrap_or(unit.get_name() + "#" + &y_dims
                .iter()
                .map(|dim| format!("{}:{}",&dim.dim_type, &dim.dim_name))
                .collect::<Vec<String>>()
                .join("#")
            );


        let bom_dim_id = format!("{}:{}#latest", x_dim.dim_type, x_dim.dim_name);

        let x = Root{
            _id: format!("{}#{}#latest", x_dim.dim_name, bom_atom_id),
            hash: unit_blob_sha.clone(),
            atom: Atom{
                unit: unit.get_name(),
                x: x_dim.dim_name,
                y: y_dims
                    .iter()
                    .map(|dim| format!("{}:{}", &dim.dim_type, &dim.dim_name))
                    .collect::<Vec<String>>()
                    .join("#"),
                version: "latest".into(),
            },
            unit: Struct{
                sha: unit_commit_sha.clone(),
                hash: unit_blob_sha.clone(),
            },
            inventory: Struct{
                sha: inventory_sha.clone(),
                hash: inventory_blob_sha.clone(),
            },
            vars: Vars{
                values: env_vars.clone().unwrap(),
                hash: env_vars_blob_sha.clone(),
            },
            timestamp: timestamp as i64,
            datetime: hr_time.to_string(),
            hashes: Hashes{
                unit: unit_blob_sha.clone(),
                dims: inventory_blob_sha.clone(),
                vars: env_vars_blob_sha.clone(),
            },
            refs: Refs{
                unit: unit.get_name(),
                dims: inventory_blob_sha.clone(),
                vars: env_vars.clone().unwrap(),
            }
        };

        dbg!(x);

        Self {
            unit_name: Some(unit.get_name()),
            unit_sha: Some(unit_commit_sha),
            unit_blob_sha: Some(unit_blob_sha),
            inventory_sha: Some(inventory_sha),
            inventory_blob_sha: Some(inventory_blob_sha),
            timestamp: Some(timestamp),
            datetime: Some(hr_time.to_string()),
            env_vars,
            env_vars_blob_sha: Some(env_vars_blob_sha),
            bom_atom_blob_sha: Some(bom_atom_blob_sha),
            bom_atom_id: Some(bom_atom_id),
            bom_dim_id: Some(bom_dim_id),
        }
    }

    /// Inserts a BOM entry into the MongoDB collection for the specified organization.
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
            let data = json!(self);
            let doc = mongodb::bson::to_bson(&data)?;
            col.insert_one(doc).run()?;
            return Ok(());
        }
        anyhow::bail!("Can't connect to bom DB");
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Vars {
    pub values: HashMap<String, String>,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Struct {
    pub sha: String,
    pub hash: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Refs {
    pub unit: String,
    pub dims: String,
    pub vars: HashMap<String, String>
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Hashes {
    pub unit: String,
    pub dims: String,
    pub vars: String
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Atom {
    pub unit: String,
    pub x: String,
    pub y: String,
    pub version: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct Root {
    pub _id: String,
    pub hash: String,
    pub atom: Atom,
    pub unit: Struct,
    pub hashes: Hashes,
    pub refs: Refs,
    pub inventory: Struct,
    pub vars: Vars,
    pub timestamp: i64,
    pub datetime: String,
}