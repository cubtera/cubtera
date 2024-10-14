pub mod data;
use data::*;

use crate::prelude::*;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use rocket::http::ext::IntoCollection;

#[derive(Debug, Clone, Default)]
pub struct Dim {
    pub dim_name: String,
    pub dim_type: String,
    pub key_path: PathBuf,
    dim_path: PathBuf,
    pub parent: Option<Box<Dim>>,
    data: Value,
}

impl Dim {
    pub fn get_data(&self) -> &Value {
        &self.data
    }

    pub fn get_data_mut(&mut self) -> &mut Value {
        &mut self.data
    }

    // Generate json with dim data
    pub fn get_dim_data(&self) -> Value {
        self.data.clone()
    }

    // Generate list of dimensions with all parents (recursively)
    pub fn get_dim_tree(&self) -> Vec<String> {
        let mut dim_tree = vec![self.dim_name.clone()];
        if let Some(parent) = &self.parent {
            dim_tree.extend(parent.get_dim_tree());
        }
        dim_tree
    }

    // Save all not json files from dimension folder to a path (usually temp folder for a unit)
    pub fn save_dim_includes(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let files = std::fs::read_dir(&self.dim_path)?;
        let files_chain = std::fs::read_dir(&self.dim_path)?;
        files
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() != "json")
            .filter_map(|file| {
                file.clone()
                    .file_stem()
                    .and_then(std::ffi::OsStr::to_str)
                    .filter(|file_name| file_name.starts_with(".default:"))
                    .map(|_| file)
            })
            .chain(
                files_chain
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|entry| entry.is_file())
                    .filter(|entry| entry.extension().unwrap_or_default() != "json")
                    .filter_map(|file| {
                        file.clone()
                            .file_stem()
                            .and_then(std::ffi::OsStr::to_str)
                            .filter(|file_name| {
                                file_name.starts_with(&format!("{}:", self.dim_name))
                            })
                            .map(|_| file)
                    }),
            )
            .try_for_each(|file: std::path::PathBuf| -> std::io::Result<()> {
                let file_name = file.clone();
                let file_name = file_name.file_name().unwrap_or_default();
                let file_name = file_name.to_str().unwrap_or_default().split(':').last();
                if file_name.is_some() && file_name.unwrap().ne("") {
                    std::fs::copy(file, path.join(file_name.unwrap()))?;
                }
                Ok(())
            })
    }

    // Save dimension folders from inventory to a path (usually temp folder for a unit)
    pub fn save_dim_folders(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let files = std::fs::read_dir(&self.dim_path)?;
        let files_chain = std::fs::read_dir(&self.dim_path)?;
        files
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_dir())
            .filter_map(|dir| {
                dir.clone()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .filter(|dir_name| dir_name.starts_with(".default:"))
                    .map(|_| dir)
            })
            .chain(
                files_chain
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.path())
                    .filter(|entry| entry.is_dir())
                    .filter_map(|dir: PathBuf| {
                        dir.clone()
                            .file_name()
                            .and_then(std::ffi::OsStr::to_str)
                            .filter(|dir_name| dir_name.starts_with(&format!("{}:", self.dim_name)))
                            .map(|_| dir)
                    }),
            )
            .try_for_each(|dir: std::path::PathBuf| -> std::io::Result<()> {
                let dir_name = dir.clone();
                let dir_name = dir_name.file_name().unwrap_or_default();
                let dir_name = dir_name.to_str().unwrap_or_default().split(':').last();
                if let Some(dir_name) = dir_name {
                    if !dir_name.is_empty() {
                        copy_folder(dir, &path.join(dir_name), true);
                    }
                }
                Ok(())
            })
    }

    // Save dimension variables values to json file
    pub fn save_json_dim_vars(&self, path: PathBuf) -> Result<String, std::io::Error> {
        let json_content = self.get_json_dim_vars();
        let json_vars_file_name = format!("cubtera_dim_{}.json", &self.dim_type);
        let json_vars_file_path = path.join(&json_vars_file_name);
        std::fs::write(
            json_vars_file_path,
            serde_json::to_string_pretty(&json_content).unwrap(),
        )?;
        Ok(json_vars_file_name)
    }

    // Generate dimension variables json values from dim values + parent dim values
    fn get_json_dim_vars(&self) -> Value {
        let mut json_vars = json!({});
        if let Some(obj) = self.data.as_object() {
            for (key, value) in obj {
                let data_key = format!("dim_{}_{}", &self.dim_type, key);
                json_vars[data_key] = value.clone();
            }
        };
        if let Some(parent) = &self.parent {
            let parent_json_vars = parent.get_json_dim_vars();
            json_vars
                .as_object_mut()
                .unwrap()
                .extend(parent_json_vars.as_object().unwrap().clone());
        }

        json_vars
    }
}

// ------------------ dim builder ------------------
pub struct DimBuilder {
    dim_name: String,
    dim_type: String,
    org: String,
    dim_path: PathBuf,
    data: Value,
    default_data: Value,
    datasource: Box<dyn DataSource>,
    storage: Storage,
}

impl Default for DimBuilder {
    fn default() -> Self {
        Self {
            dim_name: String::new(),
            dim_type: String::new(),
            org: String::new(),
            dim_path: PathBuf::new(),
            datasource: data_src_init("", "", Storage::FS),
            storage: Storage::FS,
            data: Value::Null,
            default_data: Value::Null,
        }
    }
}

impl DimBuilder {
    pub fn switch_datasource(mut self, storage: &Storage) -> Self {
        self.storage = storage.clone();
        self.datasource = data_src_init(&self.org, &self.dim_type, storage.clone());
        self
    }

    pub fn new(dim_type: &str, org: &str, storage: &Storage) -> Self {
        let datasource = data_src_init(org, dim_type, storage.clone());
        Self {
            dim_path: Path::new(&GLOBAL_CFG.inventory_path)
                .join(org)
                .join(dim_type),
            dim_type: dim_type.into(),
            org: org.into(),
            datasource,
            storage: storage.clone(),
            ..Default::default()
        }
    }

    pub fn new_from_cli(dim: &str, org: &str, storage: &Storage, context: Option<String>) -> Dim {
        let (dim_type, dim_name) = Self::split_by_colon(dim);
        Self::new(&dim_type, org, storage)
            .with_name(&dim_name)
            .with_context(context)
            .full_build()
    }

    pub fn new_undefined(dim_type: &str) -> Self {
        Self {
            dim_type: dim_type.into(),
            dim_name: "undefined".to_string(),
            data: json!({"meta": Value::Null}),
            ..Default::default()
        }
    }

    fn get_all_default_types_with_null(dim_type: &str) -> Value{
        let dim_path = Path::new(&GLOBAL_CFG.inventory_path)
            .join(&GLOBAL_CFG.org)
            .join(dim_type);
        
        let files: Vec<Value> = std::fs::read_dir(dim_path)
            .unwrap_or_exit("Unable to read inventory for optional".to_string())
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(".default:")
            )
            .map(|entry| {
                let file_name = entry.file_stem().unwrap_or_default().to_string_lossy();
                file_name.strip_prefix(".default:").unwrap_or_default().to_string()
            })
            .map(|entry| json!({
                entry : Value::Null
            }))
            .collect::<Vec<Value>>();


        json!(files)
    }

    pub fn with_name(mut self, dim_name: &str) -> Self {
        self.dim_name = dim_name.into();
        self.data["name"] = json!(self.dim_name);
        self
    }

    pub fn with_context(mut self, context: Option<String>) -> Self {
        self.datasource.set_context(context);
        self
    }

    pub fn get_all_dim_data(&self) -> Vec<Value> {
        self.datasource.get_all_data().unwrap_or_default()
    }

    pub fn get_all_kids_by_name(&self) -> HashMap<String, Vec<String>> {
        if GLOBAL_CFG.dim_relations.is_empty() {
            warn!("No dim_relations found in config");
            return HashMap::new();
        }
        let child_index = GLOBAL_CFG
            .dim_relations
            .iter()
            .position(|r| r == &self.dim_type)
            .map(|x| x + 1)
            .unwrap_or_default();
        if child_index >= GLOBAL_CFG.dim_relations.len() || child_index == 0 {
            warn!("No child dim found for {}", &self.dim_type);
            return HashMap::new();
        }
        let child_dim_type = &GLOBAL_CFG.dim_relations[child_index];
        let data = DimBuilder::new(child_dim_type, &self.org, &self.storage)
            .with_context(self.datasource.get_context())
            .get_all_dim_data()
            .into_iter()
            .filter(|data| data["name"].is_string())
            .filter(|data| {
                let parent = data["meta"]["parent"].as_str().unwrap_or_default();
                parent == format!("{}:{}", &self.dim_type, &self.dim_name)
            })
            .map(|data| data["name"].as_str().unwrap_or_default().into())
            .collect::<Vec<String>>();

        let mut kids: HashMap<String, Vec<String>> = HashMap::new();
        kids.insert(child_dim_type.into(), data);
        kids
    }

    pub fn merge_defaults(mut self) -> Self {
        let mut data = self.data.clone();
        merge_values(&mut data, &self.default_data);
        self.data = data;
        self
    }

    pub fn get_all_dim_names(&self) -> Vec<String> {
        self.datasource.get_all_names().unwrap_or_default()
    }

    pub fn full_build(self) -> Dim {
        self.read_data()
            .read_default_data()
            .merge_defaults()
            .build()
    }

    pub fn build(mut self) -> Dim {
        // ------------------ parent (optional) ------------------
        let parent = match self.data["meta"].get("parent") {
            Some(parent) => {
                let parent = parent
                    .as_str()
                    .unwrap_or_exit(format!("Parent should be a string. Got: {parent}"));
                if parent.find(':').is_none() {
                    exit_with_error(format!(
                        "Parent must be in format <parent_dim_type>:<parent_dim_name>. Got: {parent}"
                    ))
                }
                let (parent_type, parent_name) = Self::split_by_colon(parent);
                let parent_dim = DimBuilder::new(&parent_type, &self.org, &self.storage)
                    .with_name(&parent_name)
                    .with_context(self.datasource.get_context())
                    .full_build();
                Some(Box::new(parent_dim))
            }
            None => None,
        };

        // ------------------ state key path ------------------
        // recursively combine parent path with current dim path
        // schema: ".../{parent_dim_type}:{parent_dim_name}/{dim_type}:{dim_name}"
        let key_path = match parent.clone() {
            Some(parent) => parent.key_path,
            None => Path::new("").to_path_buf(),
        }
        .join(format!("{}:{}", &self.dim_type, &self.dim_name));

        self.data["name"] = Value::String(self.dim_name.clone());

        Dim {
            dim_name: self.dim_name,
            dim_type: self.dim_type,
            dim_path: self.dim_path,
            data: self.data,
            key_path,
            parent,
        }
    }

    // --------------------- data ---------------------
    pub fn get_data(&self) -> Value {
        self.data.clone()
    }

    pub fn read_data(mut self) -> Self {
        let data = self
            .datasource
            .get_data_by_name(&self.dim_name)
            .unwrap_or_default();
        data.get("meta").is_none().then(|| match self.storage {
            Storage::FS => exit_with_error(format!(
                "Can't find meta data for dimension {}:{}",
                self.dim_type, self.dim_name
            )),
            Storage::DB => {
                warn!(target: "",
                    "Can't find meta data for dimension {}:{}",
                    self.dim_type, self.dim_name
                )
            }
        });
        self.data = data.clone();
        self
    }

    pub fn save_data(&self) {
        let mut data = self.data.clone();
        data["name"] = json!(self.dim_name);
        self.datasource
            .upsert_data_by_name(&self.dim_name, data)
            .unwrap_or_exit(format!("Error saving dim {} data to DB:", &self.dim_name));
    }

    pub fn save_all_data_by_type(&self) {
        use yansi::Paint;
        let data = self.get_all_dim_data();
        let count = data.clone().len();

        let builder = DimBuilder::new(&self.dim_type, &self.org, &Storage::DB)
            .with_context(self.datasource.get_context());
        builder
            .datasource
            .upsert_all_data(data)
            .unwrap_or_exit(format!("Error saving dim {} data to DB:", &self.dim_type.red()));
        info!(target: "im", "Saved {} dimensions of {} type", count.blue(), &self.dim_type.blue());
    }

    pub fn delete_data(&self) {
        self.datasource
            .delete_data_by_name(&self.dim_name)
            .unwrap_or_exit(format!(
                "Error deleting dim {} data from DB:",
                &self.dim_name
            ));
    }

    // ------------------ default data ------------------
    pub fn get_default_data(&self) -> Value {
        self.default_data.clone()
    }

    pub fn read_default_data(mut self) -> Self {
        let data = self
            .datasource
            .get_data_by_name("_default")
            .unwrap_or_default();
        self.default_data = match self.storage {
            Storage::DB => data.get("data").cloned().unwrap_or_default(),
            Storage::FS => data,
        };
        self
    }

    pub fn save_default_data(&self) {
        let data = self.default_data.clone();

        let data = serde_json::json!({
            "data" : data
        });

        self.datasource
            .upsert_data_by_name("_default", data)
            .unwrap_or_exit(format!(
                "Error saving default data {} to DB:",
                &self.dim_type
            ));
    }

    pub fn delete_default_data(&self) {
        self.datasource
            .delete_data_by_name("_default")
            .unwrap_or_exit(format!(
                "Error deleting default data {} from DB:",
                &self.dim_type
            ));
    }

    pub fn delete_all_data_by_context(&self) {
        if let Some(context) = self.datasource.get_context() {
            self.datasource
                .delete_all_by_context(&context)
                .unwrap_or_exit(format!(
                    "Error deleting all data by context {} from DB:",
                    &self.dim_type
                ));
        }
        self.datasource
            .delete_all_by_context(&self.datasource.get_context().unwrap_or_default())
            .unwrap_or_exit(format!(
                "Error deleting all data by context {} from DB:",
                &self.dim_type
            ));
    }

    // helper methods related to DimBuilder
    fn split_by_colon(dim: &str) -> (String, String) {
        match dim.split_once(':') {
            Some((dim_type, dim_name)) => (dim_type.to_string(), dim_name.to_string()),
            None => exit_with_error(format!(
                "Dim name must be in format <dim_type>:<dim_name>. Got: {dim}"
            )),
        }
    }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use data::data_src_init;
//     use data::Storage;
//
//     #[test]
//     fn switch_datasource_changes_storage() {
//         let initial_storage = Storage::FS;
//         let new_storage = Storage::DB;
//         let dim_builder = DimBuilder::default().switch_datasource(&initial_storage);
//         let updated_dim_builder = dim_builder.switch_datasource(&new_storage);
//
//         assert_eq!(format!("{:?}", updated_dim_builder.storage), format!("{:?}", new_storage));
//     }
//
//     #[test]
//     fn switch_datasource_initializes_new_datasource() {
//         let initial_storage = Storage::FS;
//         let new_storage = Storage::DB;
//         let dim_builder = DimBuilder::default().switch_datasource(&initial_storage);
//         let updated_dim_builder = dim_builder.switch_datasource(&new_storage);
//
//         let expected_datasource = data_src_init(&updated_dim_builder.org, &updated_dim_builder.dim_type, new_storage);
//         assert_eq!(format!("{:?}", updated_dim_builder.datasource), format!("{:?}", expected_datasource));
//     }
//
//     #[test]
//     fn switch_datasource_with_same_storage() {
//         let storage = Storage::FS;
//         let dim_builder = DimBuilder::default().switch_datasource(&storage);
//         let updated_dim_builder = dim_builder.switch_datasource(&storage);
//
//         assert_eq!(updated_dim_builder.storage, storage);
//         assert_eq!(updated_dim_builder.datasource, data_src_init(&updated_dim_builder.org, &updated_dim_builder.dim_type, storage));
//     }
// }

// #[cfg(test)]
// mod tests {
//     use super::*;
//
//     #[test]
//     fn split_by_colon_with_valid_input() {
//         let input = "type:name";
//         let (dim_type, dim_name) = DimBuilder::split_by_colon(input);
//         assert_eq!(dim_type, "type");
//         assert_eq!(dim_name, "name");
//     }
//
//     #[test]
//     fn split_by_colon_with_missing_colon() {
//         let input = "typename";
//         let result = std::panic::catch_unwind(|| DimBuilder::split_by_colon(input));
//         assert!(result.is_err());
//     }
//
//     #[test]
//     fn split_by_colon_with_empty_string() {
//         let input = "";
//         let result = std::panic::catch_unwind(|| DimBuilder::split_by_colon(input));
//         assert!(result.is_err());
//     }
//
//     #[test]
//     fn split_by_colon_with_multiple_colons() {
//         let input = "type:name:extra";
//         let (dim_type, dim_name) = DimBuilder::split_by_colon(input);
//         assert_eq!(dim_type, "type");
//         assert_eq!(dim_name, "name:extra");
//     }
// }
