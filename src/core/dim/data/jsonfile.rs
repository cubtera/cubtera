#![allow(dead_code)]
use super::DataSource;
use crate::prelude::*;
use serde_json::{json, Value};
use std::{collections::HashMap, path::PathBuf};

#[derive(Debug, Clone)]
pub struct JsonDataSource {
    path: PathBuf,    // <inventory_path>/org/dim_type/
    col_name: String, // dim_type

    context: Option<String>,
}

impl JsonDataSource {
    pub fn new(org: &str, dim_type: &str, inv_path: &str) -> Self {
        let path = PathBuf::from(inv_path).join(org).join(dim_type);
        Self {
            path,
            col_name: dim_type.into(),
            context: None,
        }
    }
}

impl DataSource for JsonDataSource {
    fn get_data_by_name(&self, name: &str) -> Result<Value, Box<dyn std::error::Error>> {
        let mut filter = format!("{}{}", name, &GLOBAL_CFG.file_name_separator);
        // this replacement required for be aligned with MongoDB restriction for "." in key names
        // could be fixed by changing convention for default's file name starting with "_" instead of "."
        if filter.starts_with('_') {
            filter.replace_range(0..1, ".")
        };

        let mut data = std::fs::read_dir(&self.path)
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            .filter_map(|file| {
                // remove file extension
                file.file_stem()
                    // convert OsStr to str
                    .and_then(std::ffi::OsStr::to_str)
                    // filter names by filter from above
                    .filter(|file_name| file_name.starts_with(&filter) || *file_name == name)
                    //.filter(|file_name| !file_name.contains("schema"))
                    // convert into tuple with data type and data
                    .map(|file_name| {
                        (
                            file_name
                                // if name is equal dim name, return meta
                                .eq(name)
                                .then_some("meta")
                                // or if name is equal ".schema", return "schema"
                                .or(file_name.eq(".schema").then_some("schema"))
                                // or return the name without the filter: <name>:manifest.json -> manifest
                                .unwrap_or(file_name.trim_start_matches(&filter))
                                .to_string(),
                            read_json_file(&file).unwrap_or_exit(format!(
                                "Failed to parse data from json file: {file:?}"
                            )),
                        )
                    })
            })
            .collect::<HashMap<String, Value>>();
        data.insert("name".into(), json!(name));
        // dbg!(data.clone());
        Ok(json!(data))
    }

    // search for all dim meta files in the dim folder and return the names of the current dim type
    // and return the data for each dimension name
    fn get_all_data(&self) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
        let data = self
            .get_all_names()
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .iter()
            .map(|dim_name| self.get_data_by_name(dim_name).unwrap_or_default())
            .collect::<Vec<Value>>();

        Ok(data)
    }

    // search for all dim meta files in the dim folder and return the names of the current dim type
    // such as <name>:meta.json or <name>.json
    fn get_all_names(&self) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let meta_suffix = format!("{}meta", &GLOBAL_CFG.file_name_separator);
        let names = std::fs::read_dir(&self.path)
            .unwrap_or_exit(format!("Can't read data folder: {:?}", self.path))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            .filter_map(|file| {
                file.file_stem()
                    .and_then(std::ffi::OsStr::to_str)
                    .filter(|filename| !filename.starts_with('.'))
                    .filter(|filename| !filename.contains(&GLOBAL_CFG.file_name_separator) || filename.contains(&meta_suffix))
                    .filter(|filename| !filename.contains("schema"))
                    .map(|filename| filename.trim_end_matches(&meta_suffix).to_string())
            })
            .collect::<Vec<String>>();

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_test_file(dir: &std::path::Path, name: &str, content: &str) {
        let file_path = dir.join(name);
        let mut file = fs::File::create(file_path).unwrap();
        writeln!(file, "{}", content).unwrap();
    }

    #[test]
    fn test_get_data_by_name() {
        let dir = tempdir().unwrap();
        let org = "cubtera";
        let dim_type = "dc";
        let name = "stg1-use1";
        let json_content = r#"{ "region": "us-east-2", "vpc_cidr": "10.0.0.0/16" }"#;

        // Create test directory structure and files
        let dim_path = dir.path().join(org).join(dim_type);
        fs::create_dir_all(&dim_path).unwrap();
        create_test_file(&dim_path, &format!("{}:meta.json", name), json_content);

        let data_source = JsonDataSource::new(org, dim_type, dir.path().to_str().unwrap());
        let result = data_source.get_data_by_name(name).unwrap();

        assert_eq!(result["name"], name);
        assert_eq!(result["meta"]["region"], "us-east-2");
        assert_eq!(result["meta"]["vpc_cidr"], "10.0.0.0/16");
    }

    #[test]
    fn test_get_all_data() {
        let dir = tempdir().unwrap();
        let org = "cubtera";
        let dim_type = "dc";
        let name1 = "stg1-use1";
        let name2 = "stg1-use2";
        let json_content1 = r#"{ "region": "us-east-1", "vpc_cidr": "10.1.0.0/16" }"#;
        let json_content2 = r#"{ "region": "us-east-2", "vpc_cidr": "10.2.0.0/16" }"#;

        // Create test directory structure and files
        let dim_path = dir.path().join(org).join(dim_type);
        fs::create_dir_all(&dim_path).unwrap();
        create_test_file(&dim_path, &format!("{}.json", name1), json_content1);
        create_test_file(&dim_path, &format!("{}:meta.json", name2), json_content2);

        let data_source = JsonDataSource::new(org, dim_type, dir.path().to_str().unwrap());
        let result = data_source.get_all_data().unwrap();
        let names = result
            .iter()
            .map(|v| v["name"].as_str().unwrap())
            .collect::<Vec<&str>>();
        let meta = result
            .iter()
            .map(|v| v["meta"].clone())
            .collect::<Vec<Value>>();

        assert_eq!(result.len(), 2);
        assert!(names.contains(&name1));
        assert!(names.contains(&name2));
        assert!(meta.contains(&serde_json::from_str::<Value>(json_content1).unwrap()));
        assert!(meta.contains(&serde_json::from_str::<Value>(json_content2).unwrap()));
    }

    #[test]
    fn test_get_all_names() {
        let dir = tempdir().unwrap();
        let org = "cubtera";
        let dim_type = "dc";
        let name1 = "stg1-use1";
        let name2 = "stg1-use2";
        let json_content1 = r#"{ "region": "us-east-1", "vpc_cidr": "10.1.0.0/16" }"#;
        let json_content2 = r#"{ "region": "us-east-2", "vpc_cidr": "10.2.0.0/16" }"#;

        // Create test directory structure and files
        let dim_path = dir.path().join(org).join(dim_type);
        fs::create_dir_all(&dim_path).unwrap();
        create_test_file(&dim_path, &format!("{}:meta.json", name1), json_content1);
        create_test_file(&dim_path, &format!("{}:meta.json", name2), json_content2);

        let data_source = JsonDataSource::new(org, dim_type, dir.path().to_str().unwrap());
        let result = data_source.get_all_names().unwrap();

        assert_eq!(result.len(), 2);
        assert!(result.contains(&name1.to_string()));
        assert!(result.contains(&name2.to_string()));
    }

    #[test]
    fn test_get_all_types() {
        let dir = tempdir().unwrap();
        let org = "cubtera";
        let dim_type1 = "dc";
        let dim_type2 = "env";
        let dim_type3 = "dome";

        // Create test directory structure and files
        let dim_path1 = dir.path().join(org).join(dim_type1);
        let dim_path2 = dir.path().join(org).join(dim_type2);
        let dim_path3 = dir.path().join(org).join(dim_type3);
        fs::create_dir_all(&dim_path1).unwrap();
        fs::create_dir_all(&dim_path2).unwrap();
        fs::create_dir_all(&dim_path3).unwrap();

        let data_source = JsonDataSource::new(org, dim_type1, dir.path().to_str().unwrap());
        let result = data_source.get_all_types().unwrap();

        assert_eq!(result.len(), 3);
        assert!(result.contains(&dim_type1.to_string()));
        assert!(result.contains(&dim_type2.to_string()));
        assert!(result.contains(&dim_type3.to_string()));
    }
}
