#![allow(dead_code)]
use super::DataSource;
use crate::prelude::*;
use mongodb::bson::{doc, Bson};
use mongodb::sync::Client;
use mongodb::sync::Collection;
use mongodb::sync::Database;
use serde_json::{json, Value};

#[derive(Debug, Clone)]
pub struct MongoDBDataSource {
    client: Client,
    db_name: String,  // org
    col_name: String, // dim_type

    col: Collection<Bson>,
    db: Database,

    context: Option<String>,
}

impl MongoDBDataSource {
    pub fn new(org: &str, dim_type: &str) -> Self {
        match GLOBAL_CFG.db_client.as_ref() {
            Some(client) => {
                let db_name = org.to_string();
                let col_name = dim_type.to_string();
                let db = client.database(&db_name);
                let col = db.collection::<Bson>(&col_name);
                Self {
                    client: client.clone(),
                    col,
                    db,
                    db_name,
                    col_name,
                    context: None,
                }
            }
            None => exit_with_error("No DB config found".to_string()),
        }
    }
}

impl DataSource for MongoDBDataSource {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let options = mongodb::options::FindOneOptions::builder()
            .max_time(std::time::Duration::from_secs(5))
            .build();

        if let Some(ctx) = &self.context {
            if !ctx.is_empty() {
                let filter = doc! { "name": name, "context": ctx };
                let bson = self.col.find_one(filter, options.clone())?;
                if bson.is_some() {
                    let bson = bson.unwrap();
                    let mut res = mongodb::bson::from_bson::<Value>(bson)?;
                    res.as_object_mut().unwrap().remove("_id");
                    //TODO: remove comment after testing
                    //res.as_object_mut().unwrap().remove("context");
                    return Ok(res);
                }
            }
        }

        let filter = doc! { "name": name, "context": { "$exists": false }};
        let bson = self.col.find_one(filter, options)?;

        let res = match bson {
            Some(bson) => bson,
            None => return Ok(json!({})),
        };

        let mut res = mongodb::bson::from_bson::<Value>(res)?;
        res.as_object_mut().unwrap().remove("_id");
        Ok(res)
    }

    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let filter = doc! {
            "context": { "$exists": false },
            "name": { "$not": { "$regex": "^_default", "$options": "i" }}
        };
        let options = mongodb::options::FindOptions::builder()
            .max_time(std::time::Duration::from_secs(5))
            .build();
        let bson = self.col.find(filter, options)?;
        let res = bson
            .map(|doc| doc.unwrap())
            .map(|doc| mongodb::bson::from_bson::<Value>(doc).unwrap_or_default())
            .collect::<Vec<Value>>();
        Ok(res)
    }

    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let data = self
            .get_all_data()?
            .iter()
            //.filter(|doc| doc["context"].is_null())
            .map(|doc| doc["name"].as_str().unwrap_or_default().to_string())
            .filter(|name| !name.eq(""))
            //.filter(|name| !name.starts_with("_default:"))
            .collect::<Vec<String>>();

        Ok(data)
    }

    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let filter = doc! {};
        Ok(self.db.list_collection_names(filter)?)
    }

    fn upsert_all_data(&self, data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>> {
        let mut session = self.client.start_session(None)?;
        let mut data = data;
        data.iter_mut().for_each(|doc| {
            if let Some(name) = doc.get("name").and_then(|name| name.as_str()) {
                let mut query = doc! { "name": name, "context": { "$exists": false } };
                if let Some(ctx) = &self.context {
                    if !ctx.is_empty() {
                        query = doc! { "name": name, "context": ctx };
                        doc["context"] = json!(ctx);
                    }
                }
                let replacement = mongodb::bson::to_bson(&doc).unwrap_or_default();
                let options = mongodb::options::ReplaceOptions::builder()
                    .upsert(true)
                    .build();
                rocket::tokio::task::block_in_place(|| {
                    let res = self.col.replace_one_with_session(
                        query,
                        replacement,
                        options,
                        &mut session,
                    );
                    if res.is_err() {
                        log::debug!("Upsert failed: {:?}", res.err());
                    }
                });
            }
        });

        Ok(())
    }

    fn upsert_data_by_name(
        &self,
        name: &str,
        data: Value,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut data = data;
        data["name"] = json!(name);

        let mut query = doc! { "name": name, "context": { "$exists": false } };
        if let Some(ctx) = &self.context {
            if !ctx.is_empty() {
                query = doc! { "name": name, "context": ctx };
                data["context"] = json!(ctx);
            }
        }

        let replacement = mongodb::bson::to_bson(&data)?;
        let options = mongodb::options::ReplaceOptions::builder()
            .upsert(true)
            .build();
        let _ = self.col.replace_one(query, replacement, options)?;
        Ok(())
    }

    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut filter = doc! { "name": name, "context": { "$exists": false } };
        if let Some(ctx) = &self.context {
            if !ctx.is_empty() {
                filter = doc! { "name": name, "context": ctx };
            }
        }
        let _ = self.col.delete_one(filter, None)?;
        Ok(())
    }

    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>> {
        let db_names = &GLOBAL_CFG
            .db_client
            .as_ref()
            .unwrap()
            .list_database_names(None, None)?;
        db_names.iter()
            .for_each(|name| {
                let db = GLOBAL_CFG.db_client.as_ref().unwrap().database(name);
                db.list_collection_names(None).unwrap()
                    .iter()
                    .for_each(|col| {
                        let col = db.collection::<Bson>(col);
                        let filter = doc! { "context": context };
                        let res = col.delete_many(filter, None);
                        //count deleted
                        if let Ok(res) = res {
                            if res.deleted_count > 0 {
                                log::info!(target: "", "Deleted {} docs from {} with context {context}.", res.deleted_count, col.name());
                            }
                        }
                    });
            });
        Ok(())
    }

    fn set_context(&mut self, context: Option<String>) {
        self.context = context;
    }

    fn get_context(&self) -> Option<String> {
        self.context.clone()
    }

    // TODO: Remove after usage check
    // Legacy method for dim defaults
    // ! Doesn't support context
    // fn get_data_dim_defaults(&self) -> Result<Value, Box<dyn std::error::Error>> {
    //     let filter = doc! { "name": self.col_name.clone(), "context": { "$exists": false } };
    //     let options = mongodb::options::FindOneOptions::builder()
    //         .max_time(std::time::Duration::from_secs(5))
    //         .build();
    //     let col = self.db.collection::<Bson>("defaults");
    //     let bson = col.find_one(filter, options)?;
    //     let res = match bson {
    //         Some(bson) => bson,
    //         None => return Ok(json!({})),
    //     };
    //     let mut res = mongodb::bson::from_bson::<Value>(res)?;
    //     let data = res["defaults"].take();
    //     Ok(data)
    // }

    // Legacy method for dim defaults
    // ! Doesn't support context
    // fn upsert_data_dim_defaults(&self, name: &str, data: Value) -> Result<(), Box<dyn std::error::Error>> {
    //     let query = doc! { "name": name, "context": { "$exists": false } };
    //     let data = serde_json::json!({
    //         "name" : name,
    //         "defaults" : data
    //     });
    //
    //     let col = self.db.collection::<Bson>("defaults");
    //     let replacement = mongodb::bson::to_bson(&data)?;
    //     let options = mongodb::options::ReplaceOptions::builder().upsert(true).build();
    //     let _ = col.replace_one(query, replacement, options)?;
    //
    //     Ok(())
    // }

    // Legacy method for dim defaults
    // ! Doesn't support context
    // fn delete_data_dim_defaults(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
    //     let filter = doc! { "name": name, "context": { "$exists": false } };
    //     let col = self.db.collection::<Bson>("defaults");
    //     let _ = col.delete_one(filter, None)?;
    //     Ok(())
    // }
}
