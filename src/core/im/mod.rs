use crate::core::dim::*;
use crate::core::dim::data::*;
use crate::utils::helper::*;
use crate::prelude::GLOBAL_CFG;

use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;

pub fn get_dim_by_name(
    dim_type: &str,
    dim_name: &str,
    org: &str,
    storage: &Storage,
    context: Option<String>
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

pub fn get_dim_names_by_type(
    dim_type: &str,
    org: &str,
    storage: &Storage
) -> Value {
    let dims = DimBuilder::new(dim_type, org, storage)
        .get_all_dim_names();

    json!({
        "status": "ok",
        "id": "dimsByType",
        "type": dim_type,
        "data": dims,
    })
}

pub fn get_dims_data_by_type(dim_type: &str, org: &str, storage: &Storage) -> Value {
    let names = DimBuilder::new(dim_type, org, storage)
        .get_all_dim_names();

    let data = names.iter()
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

pub fn get_dim_kids (
    dim_type: &str,
    dim_name: &str,
    org: &str,
    storage: &Storage
) -> Value {
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
            let db = GLOBAL_CFG.db_client.clone().unwrap_or_exit(
                "Can't start DB client".into()
            );
            db.list_database_names(None, None).unwrap_or_exit(
                "Can't read list of orgs from DB".into()
            )
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
    let keys: HashMap<String, String> = keys.iter()
        .map(|dim: &String| {
            let parts: Vec<&str> = dim.split(':').collect();
            (parts[0].to_string(), parts[1].to_string())
        })
        .collect();
    let filter = json!(keys);

    get_dlog(org, filter.clone(), limit)
}

pub fn get_dlog(org: &str, filter: Value, limit: Option<i64>) -> Value {
    let client: Option<mongodb::sync::Client> = GLOBAL_CFG.dlog_db.as_ref().map(|db| db_connect(db));

    if client.is_none() {
        return json!({
            "status": "error",
            "id": "dlog",
            "message": format!("Can't connect to dlog DB {}", &GLOBAL_CFG.dlog_db.clone().unwrap_or("".to_string())),
            "notes": "Check CUBTERA_DLOG_DB env variable",
            "data": Value::Null,
        })
    }

    let db = client.unwrap().database(org);
    let col = db.collection::<mongodb::bson::Bson>("dlog");
    //let col = MongoCollection::new(org, "dlog", &client.unwrap());
    let options = mongodb::options::FindOptions::builder()
        .sort(mongodb::bson::doc! { "timestamp": -1 })
        .limit(limit.unwrap_or(10))
        .build();
    let filter = to_dot_notation(&filter, "".to_string());
    let filter = json!(filter);
    let filter= mongodb::bson::to_document(&filter).unwrap_or_default();

    let res = col.find(filter.clone(), options).unwrap();
    let res:Vec<Value>  = res.collect::<mongodb::error::Result<Vec<mongodb::bson::Bson>>>()
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
            let new_key = if prefix.is_empty() { key.clone() } else { format!("{}.{}", prefix, key) };
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

pub fn get_all_dim_types(org: &str, storage: &Storage) -> Value {
    let dim_types = match storage {
        Storage::DB => {
            let client = GLOBAL_CFG.db_client.clone().unwrap();
            let db = client.database(org);
            db.list_collection_names(None).unwrap()
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
