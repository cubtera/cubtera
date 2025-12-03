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
        if let Some(ctx) = &self.context {
            if !ctx.is_empty() {
                let filter = doc! { "name": name, "context": ctx };
                let bson = self.col.find_one(filter)
                    .run()?;
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
        let bson = self.col.find_one(filter).run()?;

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
        let bson = self.col.find(filter).run()?;
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
        Ok(self.db.list_collection_names().run()?)
    }

    fn upsert_all_data(&self, data: Vec<Value>) -> Result<(), Box<dyn std::error::Error>> {
        // let mut session = self.client.start_session()?;
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
                rocket::tokio::task::block_in_place(|| {
                    let res = self.col.replace_one(
                        query,
                        replacement,
                    )
                        .upsert(true)
                        .run();
                    if res.is_err() {
                        debug!("Upsert failed: {:?}", res.err());
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
        let _ = self.col.replace_one(query, replacement)
            .upsert(true)
            .run()?;
        Ok(())
    }

    fn delete_data_by_name(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let mut filter = doc! { "name": name, "context": { "$exists": false } };
        if let Some(ctx) = &self.context {
            if !ctx.is_empty() {
                filter = doc! { "name": name, "context": ctx };
            }
        }
        let _ = self.col.delete_one(filter).run()?;
        Ok(())
    }

    fn delete_all_by_context(&self, context: &str) -> Result<(), Box<dyn std::error::Error>> {
        let db_names = &GLOBAL_CFG
            .db_client
            .as_ref()
            .unwrap()
            .list_database_names().run()?;
        db_names.iter()
            .for_each(|name| {
                let db = GLOBAL_CFG.db_client.as_ref().unwrap().database(name);
                db.list_collection_names().run().unwrap()
                    .iter()
                    .for_each(|col| {
                        let col = db.collection::<Bson>(col);
                        let filter = doc! { "context": context };
                        let res = col.delete_many(filter).run();
                        //count deleted
                        if let Ok(res) = res {
                            if res.deleted_count > 0 {
                                info!(target: "", "Deleted {} docs from {} with context {context}.", res.deleted_count, col.name());
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
}
