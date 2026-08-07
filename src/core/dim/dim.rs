use serde_json::{json, Value};
use std::path::PathBuf;

/// Core dimension structure representing a single dimension with its data and relationships
#[derive(Debug, Clone, Default)]
pub struct Dim {
    pub dim_name: String,
    pub dim_type: String,
    pub key_path: PathBuf,
    pub(crate) dim_path: PathBuf,
    pub parent: Option<Box<Dim>>,
    pub(crate) data: Value,
    pub data_sha: String,
    pub kids: Option<Vec<String>>,
}

impl Dim {
    /// Get immutable reference to dimension data
    pub fn get_data(&self) -> &Value {
        &self.data
    }

    /// Get mutable reference to dimension data
    pub fn get_data_mut(&mut self) -> &mut Value {
        &mut self.data
    }

    /// Generate json with dim data (clone of internal data)
    pub fn get_dim_data(&self) -> Value {
        self.data.clone()
    }

    /// Generate list of dimensions with all parents (recursively)
    /// Returns a vector starting with current dimension, followed by parents
    pub fn get_dim_tree(&self) -> Vec<String> {
        let mut dim_tree = vec![self.dim_name.clone()];
        if let Some(parent) = &self.parent {
            dim_tree.extend(parent.get_dim_tree());
        }
        dim_tree
    }

    /// Save dimension variables values to json file
    /// Returns the filename of the created JSON file
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

    /// Generate dimension variables json values from dim values + parent dim values
    /// Creates a flattened JSON object with prefixed keys for each dimension level
    pub(crate) fn get_json_dim_vars(&self) -> Value {
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