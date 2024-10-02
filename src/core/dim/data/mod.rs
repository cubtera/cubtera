mod jsonfile;
mod mongodb;

use crate::globals::GLOBAL_CFG;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Storage {
    FS,
    DB,
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

    // TODO: Remove after usage check
    // Legacy defaults
    // fn get_data_dim_defaults(&self) -> Result<Value, Box<dyn std::error::Error>>;
    // fn upsert_data_dim_defaults(&self, name: &str, data: Value) -> Result<(), Box<dyn std::error::Error>>;
    // fn delete_data_dim_defaults(&self, name: &str) -> Result<(), Box<dyn std::error::Error>>;
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
