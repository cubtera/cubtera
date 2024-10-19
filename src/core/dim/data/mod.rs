mod jsonfile;
mod mongodb;

use crate::globals::GLOBAL_CFG;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Storage {
    FS,
    DB,
}

impl Default for Storage {
    fn default() -> Self {
        Storage::FS
    }
}

impl Storage {
    pub fn from_str(s: &str) -> Self {
        match s {
            "fs" => Storage::FS,
            "db" => Storage::DB,
            _ => unreachable!("Unknown storage type"),
        }
    }

    pub fn to_str(&self) -> &str {
        match self {
            Storage::FS => "fs",
            Storage::DB => "db",
        }
    }

    // TODO: make configurable
    pub fn get_defaults_prefix(&self) -> &str {
        match self {
            Storage::FS => ".defaults:",
            Storage::DB => "_defaults:",
        }
    }
}

pub trait CloneBox {
    fn clone_box(&self) -> Box<dyn DataSource>;
}

impl<T: DataSource + Clone> CloneBox for T {
    fn clone_box(&self) -> Box<dyn DataSource> {
        Box::new(self.clone())
    }
}

pub trait DataSource: CloneBox + 'static {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>>;
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>>;
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;
    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>>;

    // only for DB (json files are source of truth)
    fn upsert_all_data(&self, _data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!("this data source doesn't support upsert_all_data");
        Ok(())
    }

    fn upsert_data_by_name(
        &self,
        name: &str,
        data: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support upsert_data_by_name: {}: {}",
            name,
            serde_json::json!(data)
        );
        Ok(())
    }

    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support delete_data_by_name: {}",
            name
        );
        Ok(())
    }

    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!(
            "this data source doesn't support delete_all_by_context: {}",
            context
        );
        Ok(())
    }

    fn set_context(&mut self, context: Option<String>);
    fn get_context(&self) -> Option<String>;
}

pub fn data_src_init(org: &str, dim_type: &str, storage: Storage) -> Box<dyn DataSource> {
    match storage {
        Storage::DB => Box::new(mongodb::MongoDBDataSource::new(org, dim_type)),
        Storage::FS => Box::new(jsonfile::JsonDataSource::new(
            org,
            dim_type,
            &GLOBAL_CFG.inventory_path,
        )),
        //_ => unreachable!("Unknown storage type")
    }
}

// pub enum DataSrc {
//     MongoDB(mongodb::MongoDBDataSource),
//     Json(jsonfile::JsonDataSource),
//     // Add other data source types here...
// }

// impl Clone for DataSrc {
//     fn clone(&self) -> Self {
//         match self {
//             DataSrc::MongoDB(ds) => DataSrc::MongoDB(ds.clone()),
//             DataSrc::Json(ds) => DataSrc::Json(ds.clone()),
//             // Handle other data source types here...
//         }
//     }
// }
