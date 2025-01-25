pub mod data;
use data::*;

use crate::prelude::*;

use serde_json::{json, Value};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct Dim {
    pub dim_name: String,
    pub dim_type: String,
    pub key_path: PathBuf,
    dim_path: PathBuf,
    pub parent: Option<Box<Dim>>,
    data: Value,
    pub data_sha: String,
    pub kids: Option<Vec<String>>,
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
        let entry_filter =
            |entry: &Path| entry.is_file() && entry.extension().unwrap_or_default() != "json";

        self.process_dim_entries(path, entry_filter)
    }

    // Save dimension folders from inventory to a path (usually temp folder for a unit)
    pub fn save_dim_folders(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let entry_filter = |entry: &Path| entry.is_dir();

        self.process_dim_entries(path, entry_filter)
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

    fn get_filtered_entries<F>(
        &self,
        prefix: &str,
        entry_filter: F,
    ) -> std::io::Result<Vec<PathBuf>>
    where
        F: Fn(&Path) -> bool,
    {
        Ok(std::fs::read_dir(&self.dim_path)?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(move |entry| entry_filter(entry))
            .filter_map(move |entry| {
                entry
                    .clone()
                    .file_name()
                    .and_then(std::ffi::OsStr::to_str)
                    .filter(|name| name.starts_with(prefix))
                    .map(|_| entry)
            })
            .collect())
    }

    fn process_dim_entries<F>(&self, path: PathBuf, entry_filter: F) -> Result<(), std::io::Error>
    where
        F: Fn(&Path) -> bool,
    {
        let default_prefix = format!(".default{}", &GLOBAL_CFG.file_name_separator);
        let dim_prefix = format!("{}{}", &self.dim_name, &GLOBAL_CFG.file_name_separator);

        let default_entries = self.get_filtered_entries(&default_prefix, &entry_filter)?;
        let dim_entries = self.get_filtered_entries(&dim_prefix, &entry_filter)?;

        default_entries
            .into_iter()
            .chain(dim_entries)
            .try_for_each(|entry| {
                let entry_name = entry
                    .file_name()
                    .and_then(|f| f.to_str())
                    .and_then(|s| s.split(&GLOBAL_CFG.file_name_separator).last())
                    .filter(|s| !s.is_empty());

                match entry_name {
                    Some(name) => self.copy_entry(&entry, &path.join(name), entry.is_dir()),
                    None => Ok(()),
                }
            })
    }

    fn copy_entry(&self, src: &Path, dest: &Path, is_dir: bool) -> std::io::Result<()> {
        if is_dir {
            copy_folder(src.to_path_buf(), &dest.to_path_buf(), true);
            Ok(())
        } else {
            std::fs::copy(src, dest).map(|_| ())
        }
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
        let storage = match &GLOBAL_CFG.db_client {
            Some(_) => Storage::DB,
            None => Storage::FS,
        };

        let dim = Self::new(dim_type, &GLOBAL_CFG.org, &storage).read_default_data();

        let null_data: Value = dim
            .default_data
            .clone()
            .as_object_mut()
            .unwrap_or(&mut serde_json::Map::new())
            .keys()
            .map(|key| key.to_string())
            .filter(|key| !key.starts_with("name"))
            .map(|key| (key, Value::Null))
            .collect();

        Self {
            dim_type: dim_type.into(),
            dim_name: "undefined".to_string(),
            data: null_data,
            ..Default::default()
        }
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
            warn!(
                "No dim_relations found in config for {}:{}",
                &self.dim_type, &self.dim_name
            );
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

        let kids: Vec<String> = self
            .get_all_kids_by_name()
            .into_iter()
            .flat_map(|(k, v)| v.into_iter().map(move |x| format!("{}:{}", k, x)))
            .collect();

        // ------------------ state key path ------------------
        // recursively combine parent path with current dim path
        // schema: ".../{parent_dim_type}:{parent_dim_name}/{dim_type}:{dim_name}"
        let key_path = match parent.clone() {
            Some(parent) => parent.key_path,
            None => Path::new("").to_path_buf(),
        }
        .join(format!("{}:{}", &self.dim_type, &self.dim_name));

        self.data["name"] = Value::String(self.dim_name.clone());

        let data_sha = get_sha_by_value(&self.data);

        Dim {
            dim_name: self.dim_name,
            dim_type: self.dim_type,
            dim_path: self.dim_path,
            data: self.data,
            data_sha,
            key_path,
            parent,
            kids: kids.is_empty().not().then(|| kids),
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
            .unwrap_or_exit(format!(
                "Error saving dim {} data to DB:",
                &self.dim_type.red()
            ));
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
        // let defaults_name = self.storage.get_defaults_prefix();
        let data = self
            .datasource
            .get_data_by_name("_default")
            .unwrap_or_default();

        // TODO: remove "data" key usage and read default data as is
        // self.default_data = data;
        self.default_data = match self.storage {
            Storage::DB => data.get("data").cloned().unwrap_or_default(),
            Storage::FS => data,
        };

        self
    }

    pub fn save_default_data(&self) {
        let data = self.default_data.clone();

        // TODO: remove "data" key usage and save default data as is
        let data = json!({
            "data" : data
        });

        self.datasource
            .upsert_data_by_name("_default", data)
            .unwrap_or_exit(format!(
                "Error saving default data {} to DB",
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    // Helper function to create a basic Dim instance for testing
    fn create_test_dim() -> Dim {
        Dim {
            dim_name: "test_dim".to_string(),
            dim_type: "test_type".to_string(),
            key_path: PathBuf::from("test/path"),
            dim_path: PathBuf::from("test/dim/path"),
            parent: None,
            data: json!({"test_key": "test_value"}),
            data_sha: "test_sha".to_string(),
            kids: Some(vec!["child1".to_string(), "child2".to_string()]),
        }
    }

    #[test]
    fn test_dim_data_access() {
        let dim = create_test_dim();
        // Test get_data
        assert_eq!(dim.get_data()["test_key"], "test_value");
        // Test get_dim_data
        assert_eq!(dim.get_dim_data()["test_key"], "test_value");
    }

    #[test]
    fn split_by_colon_with_valid_input() {
        let input = "type:name";
        let (dim_type, dim_name) = DimBuilder::split_by_colon(input);
        assert_eq!(dim_type, "type");
        assert_eq!(dim_name, "name");
    }

    #[test]
    fn split_by_colon_with_multiple_colons() {
        let input = "type:name:extra";
        let (dim_type, dim_name) = DimBuilder::split_by_colon(input);
        assert_eq!(dim_type, "type");
        assert_eq!(dim_name, "name:extra");
    }

    // TODO: unwind can't handle exit(1) cases. Fix.
    // #[test]
    // fn split_by_colon_with_empty_string() {
    //     let input = "";
    //     let result = std::panic::catch_unwind(|| DimBuilder::split_by_colon(input));
    //     //assert!(result.is_err());
    //     assert_eq!(format!("{:?}", result), "Dim name must be in format <dim_type>:<dim_name>. Got: ");
    // }
    // #[test]
    // fn split_by_colon_with_missing_colon() {
    //     let input = "typename";
    //     let result = std::panic::catch_unwind(|| DimBuilder::split_by_colon(input));
    //     assert!(result.is_err());
    // }

    #[test]
    fn test_dim_tree_generation() {
        let mut child_dim = create_test_dim();
        let mut parent_dim = Dim {
            dim_name: "parent_dim".to_string(),
            dim_type: "parent_type".to_string(),
            ..create_test_dim()
        };

        // Create grandparent
        let grandparent_dim = Dim {
            dim_name: "grandparent_dim".to_string(),
            dim_type: "grandparent_type".to_string(),
            ..create_test_dim()
        };

        parent_dim.parent = Some(Box::new(grandparent_dim));
        child_dim.parent = Some(Box::new(parent_dim));

        let tree = child_dim.get_dim_tree();
        assert_eq!(tree, vec!["test_dim", "parent_dim", "grandparent_dim"]);
    }

    #[test]
    fn test_dim_builder_creation() {
        let builder = DimBuilder::new("test_type", "test_org", &Storage::FS);
        assert_eq!(builder.dim_type, "test_type");
        assert_eq!(builder.org, "test_org");
    }

    #[test]
    fn test_dim_builder_with_name() {
        let builder = DimBuilder::new("test_type", "test_org", &Storage::FS).with_name("test_name");

        assert_eq!(builder.dim_name, "test_name");
        assert_eq!(builder.data["name"], "test_name");
    }

    #[test]
    fn test_dim_builder_merge_defaults() {
        let mut builder = DimBuilder::new("test_type", "test_org", &Storage::FS);
        builder.default_data = json!({
            "default_key": "default_value",
            "shared_key": "default_shared"
        });
        builder.data = json!({
            "data_key": "data_value",
            "shared_key": "data_shared"
        });

        let merged = builder.merge_defaults();
        assert_eq!(merged.data["default_key"], "default_value");
        assert_eq!(merged.data["data_key"], "data_value");
        assert_eq!(merged.data["shared_key"], "data_shared");
    }

    #[test]
    fn test_get_data_mut() {
        let mut dim = Dim::default();
        let data = Value::String("test".to_string());
        dim.data = data.clone();
        assert_eq!(dim.get_data_mut(), &data);
    }

    #[test]
    fn test_get_data() {
        let mut dim = Dim::default();
        let data = Value::String("test".to_string());
        dim.data = data.clone();
        assert_eq!(dim.get_data(), &data);
    }

    #[test]
    fn test_get_dim_data() {
        let mut dim = Dim::default();
        let data = Value::String("test".to_string());
        dim.data = data.clone();
        assert_eq!(dim.get_dim_data(), data);
    }

    #[test]
    fn test_get_dim_tree() {
        let mut dim = Dim::default();
        dim.dim_name = "child".to_string();
        let mut parent = Dim::default();
        parent.dim_name = "parent".to_string();
        dim.parent = Some(Box::new(parent));

        let tree = dim.get_dim_tree();
        assert_eq!(tree, vec!["child", "parent"]);
    }

    #[test]
    fn test_get_filtered_entries() {
        let mut dim = Dim::default();
        dim.dim_path = PathBuf::from("test_path");
        let entry_filter = |entry: &Path| entry.is_file();
        let result = dim.get_filtered_entries("test_prefix", entry_filter);
        assert!(result.is_err()); // Should error if test_path doesn't exist
    }

    #[test]
    fn test_dim_json_vars() {
        let mut dim = create_test_dim();
        dim.data = json!({
            "var1": "value1",
            "var2": "value2"
        });

        let temp_dir = TempDir::new().unwrap();
        let result = dim.save_json_dim_vars(temp_dir.path().to_path_buf());
        assert!(result.is_ok());
    }

    // #[test]
    // fn test_dim_builder_switch_datasource() {
    //     let builder = DimBuilder::new("test_type", "test_org", &Storage::FS);
    //     let switched = builder.switch_datasource(&Storage::DB);
    //     assert_eq!(
    //         format!("{:?}", switched.storage),
    //         format!("{:?}", Storage::DB)
    //     );
    // }

    // #[test]
    // fn test_new_undefined_dim() {
    //     let dim = DimBuilder::new_undefined("test_type").build();
    //     assert_eq!(dim.dim_name, "undefined".to_string());
    //     assert_eq!(dim.dim_type, "test_type".to_string());
    // }

    #[test]
    fn test_dim_builder_with_context() {
        let builder = DimBuilder::new("test_type", "test_org", &Storage::FS)
            .with_context(Some("test_context".to_string()));
        assert!(builder.datasource.get_context().is_some());
        assert_eq!(builder.datasource.get_context().unwrap(), "test_context");
    }
}
