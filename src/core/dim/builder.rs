use super::{Dim, data::*};
use super::error::{DimError, DimResult};
use crate::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::ops::Not;
use std::path::{Path, PathBuf};

/// Builder pattern for creating Dim instances with various configurations
pub struct DimBuilder {
    pub(crate) dim_name: String,
    pub(crate) dim_type: String,
    pub(crate) org: String,
    pub(crate) dim_path: PathBuf,
    pub(crate) data: Value,
    pub(crate) default_data: Value,
    pub(crate) datasource: Box<dyn DataSource>,
    pub(crate) storage: Storage,
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
    /// Switch the datasource to a different storage backend
    pub fn switch_datasource(mut self, storage: &Storage) -> Self {
        self.storage = storage.clone();
        self.datasource = data_src_init(&self.org, &self.dim_type, storage.clone());
        self
    }

    /// Create a new DimBuilder with specified type, organization, and storage
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

    /// Create a Dim from CLI input (exits on error)
    pub fn new_from_cli(dim: &str, org: &str, storage: &Storage, context: Option<String>) -> Dim {
        match Self::new_from_cli_safe(dim, org, storage, context) {
            Ok(dim) => dim,
            Err(e) => exit_with_error(format!("{}", e)),
        }
    }
    
    /// Safe version of new_from_cli that returns Result instead of exiting
    pub fn new_from_cli_safe(dim: &str, org: &str, storage: &Storage, context: Option<String>) -> DimResult<Dim> {
        let (dim_type, dim_name) = Self::split_by_colon_safe(dim)?;
        Ok(Self::new(&dim_type, org, storage)
            .with_name(&dim_name)
            .with_context(context)
            .full_build())
    }

    /// Create an undefined dimension with null values for all fields
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

    /// Set the dimension name
    pub fn with_name(mut self, dim_name: &str) -> Self {
        self.dim_name = dim_name.into();
        self.data["name"] = json!(self.dim_name);
        self
    }

    /// Set the context for the datasource
    pub fn with_context(mut self, context: Option<String>) -> Self {
        self.datasource.set_context(context);
        self
    }

    /// Get all dimension data from the datasource
    pub fn get_all_dim_data(&self) -> Vec<Value> {
        self.datasource.get_all_data().unwrap_or_default()
    }

    /// Get all child dimensions grouped by type
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
            debug!("No child dim found for {}", &self.dim_type);
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

    /// Merge default data with current data
    pub fn merge_defaults(mut self) -> Self {
        let mut data = self.data.clone();
        merge_values(&mut data, &self.default_data);
        self.data = data;
        self
    }

    /// Get all dimension names from the datasource
    pub fn get_all_dim_names(&self) -> Vec<String> {
        self.datasource.get_all_names().unwrap_or_default()
    }

    /// Build a complete Dim with data loading and default merging
    pub fn full_build(self) -> Dim {
        self.read_data()
            .read_default_data()
            .merge_defaults()
            .build()
    }

    /// Build a Dim instance from the current builder state
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
            key_path,
            dim_path: self.dim_path,
            parent,
            data: self.data,
            data_sha,
            kids: kids.is_empty().not().then(|| kids),
        }
    }

    // --------------------- data operations ---------------------
    
    /// Get a copy of the current data
    pub fn get_data(&self) -> Value {
        self.data.clone()
    }

    /// Read data from the datasource by dimension name
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

    /// Save current data to the datasource
    pub fn save_data(&self) {
        let mut data = self.data.clone();
        data["name"] = json!(self.dim_name);
        self.datasource
            .upsert_data_by_name(&self.dim_name, data)
            .unwrap_or_exit(format!("Error saving dim {} data to DB:", &self.dim_name));
    }

    /// Save all dimension data of this type to database
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

    /// Delete data from the datasource
    pub fn delete_data(&self) {
        self.datasource
            .delete_data_by_name(&self.dim_name)
            .unwrap_or_exit(format!(
                "Error deleting dim {} data from DB:",
                &self.dim_name
            ));
    }

    // ------------------ default data operations ------------------
    
    /// Get a copy of the default data
    pub fn get_default_data(&self) -> Value {
        self.default_data.clone()
    }

    /// Read default data from the datasource
    pub fn read_default_data(mut self) -> Self {
        let data = self
            .datasource
            .get_data_by_name("_default")
            .unwrap_or_default();

        // TODO: remove "data" key usage and read default data as is
        self.default_data = match self.storage {
            Storage::DB => data.get("data").cloned().unwrap_or_default(),
            Storage::FS => data,
        };

        self
    }

    /// Save default data to the datasource
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

    /// Delete default data from the datasource
    pub fn delete_default_data(&self) {
        self.datasource
            .delete_data_by_name("_default")
            .unwrap_or_exit(format!(
                "Error deleting default data {} from DB:",
                &self.dim_type
            ));
    }

    /// Delete all data by context from the datasource
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

    // ------------------ helper methods ------------------
    
    /// Split dimension string by colon (exits on error)
    fn split_by_colon(dim: &str) -> (String, String) {
        match Self::split_by_colon_safe(dim) {
            Ok((dim_type, dim_name)) => (dim_type, dim_name),
            Err(e) => exit_with_error(format!("{}", e)),
        }
    }
    
    /// Safe version of split_by_colon that returns Result instead of exiting
    fn split_by_colon_safe(dim: &str) -> DimResult<(String, String)> {
        match dim.split_once(':') {
            Some((dim_type, dim_name)) => {
                if dim_type.is_empty() || dim_name.is_empty() {
                    Err(DimError::invalid_format(dim))
                } else {
                    Ok((dim_type.to_string(), dim_name.to_string()))
                }
            }
            None => Err(DimError::invalid_format(dim)),
        }
    }
} 