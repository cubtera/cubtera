#![allow(dead_code)]
use super::DataSource;
use crate::prelude::*;
use serde_json::{Value, json};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone)]
pub struct JsonDataSource {
    path: PathBuf, // <inventory_path>/org/dim_type/
    col_name: String, // dim_type

    context: Option<String>
}

impl JsonDataSource {
    pub fn new(org: &str, dim_type: &str) -> Self {
        let path = PathBuf::from(&GLOBAL_CFG.inventory_path).join(org).join(dim_type);
        Self {
            path,
            col_name: dim_type.into(),
            context: None
        }
    }
}

impl DataSource for JsonDataSource {

    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let mut filter = format!("{}:", name);
        if filter.starts_with('_') {filter.replace_range(0..1, ".")};
        //dbg!(&filter);
        let mut data = std::fs::read_dir(&self.path)
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            .filter_map(|file| file.file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .filter(|file_name| file_name.starts_with(&filter) || *file_name == name)
                //.filter(|file_name| !file_name.contains("schema"))
                .map(|file_name| 
                    (
                        file_name
                            .eq(name).then_some("meta")
                            .or(file_name.eq(".schema").then_some("schema"))
                            .unwrap_or(file_name.trim_start_matches(&filter))
                            .to_string(), 
                        
                        read_json_file(&file)
                            .unwrap_or_exit(format!("Failed to parse data from json file: {file:?}"))
                    )
                )
            )
            .collect::<HashMap<String, Value>>();
        data.insert("name".into(), json!(name));
        //dbg!(data.clone());
        Ok(json!(data))
    }

    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let data = self.get_all_names()
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .iter()
            .map(|dim_name| 
                self.get_data_by_name(dim_name).unwrap_or_default()
            )
            .collect::<Vec<Value>>();

        Ok(data)
    }

    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let names = std::fs::read_dir(&self.path)
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            .filter_map(|file| file.file_stem()
                .and_then(std::ffi::OsStr::to_str)
                .filter(|filename| !filename.starts_with('.'))
                .filter(|filename| !filename.contains(':') || filename.contains(":meta"))
                .filter(|filename| !filename.contains("schema"))
                .map(|filename| filename.trim_end_matches(":meta").to_string())
            ).collect::<Vec<String>>();

        Ok(names)
    }

    fn get_all_types(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let types = std::fs::read_dir(self.path.parent().unwrap_or(self.path.as_path()))
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| path.is_dir())
            .map(|path| path.file_name().unwrap().to_str().unwrap().to_string())
            .collect::<Vec<String>>();

        Ok(types)
    }

    fn set_context(&mut self, context: Option<String>) {
        self.context = context;
    }

    fn get_context(&self) -> Option<String> {
        self.context.clone()
    }

    fn get_data_dim_defaults(&self) -> Result<Value, Box<dyn std::error::Error>> {
        let dim_type = self.path.file_name().unwrap().to_str().unwrap();
        let path = self.path
            .join(".config")
            .join(format!("{}:defaults.json", dim_type));
        let data: Value = read_json_file(&path).unwrap_or_default();

        Ok(data)
    }

    fn upsert_data_dim_defaults(&self, name: &str, data: Value) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!("json data source doesn't support upsert_data_dim_defaults: {}: {}", name, json!(data));
        Ok(())
    }

    fn delete_data_dim_defaults(&self, name: &str) -> Result<(), Box<dyn std::error::Error>> {
        log::debug!("json data source doesn't support delete_data_dim_defaults: {}", name);
        Ok(())
    }
}