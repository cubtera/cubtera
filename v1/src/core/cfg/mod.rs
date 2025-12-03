use crate::prelude::*;
use crate::utils::helper::*;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::ops::Add;
use std::path::Path;

pub static GLOBAL_CFG: Lazy<CubteraConfig> = Lazy::new(|| CubteraConfig::new().build());

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CubteraConfig {
    #[serde(default = "default_workspace_path")]
    pub workspace_path: String,
    #[serde(default = "default_inventory_path")]
    pub inventory_path: String,
    #[serde(default = "default_units_path")]
    pub units_path: String,
    #[serde(default = "default_modules_path")]
    pub modules_path: String,
    #[serde(default = "default_plugins_path")]
    pub plugins_path: String,
    #[serde(default = "default_org")]
    pub org: String,
    #[serde(default = "default_temp_folder_path")]
    pub temp_folder_path: String,
    #[serde(
        deserialize_with = "deserialize_colon_list",
        serialize_with = "serialize_colon_list",
        default = "default_orgs"
    )]
    pub orgs: Vec<String>,
    #[serde(
        deserialize_with = "deserialize_colon_list",
        serialize_with = "serialize_colon_list",
        default = "default_dim_relations"
    )]
    pub dim_relations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_db: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_user_name_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_number_env: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dlog_job_name_env: Option<String>,
    #[serde(default)]
    pub clean_cache: bool,
    #[serde(default)]
    pub always_copy_files: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<HashMap<String, HashMap<String, String>>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HashMap<String, HashMap<String, String>>>,
    #[serde(skip)]
    pub db_client: Option<mongodb::sync::Client>,
    #[serde(default = "default_file_name_separator")]
    pub file_name_separator: String,
}

fn default_workspace_path() -> String {
    "~/.cubtera/workspace".into()
}
fn default_temp_folder_path() -> String {
    "~/.cubtera/tmp".into()
}

fn default_units_folder() -> String {
    "units".into()
}
fn default_units_path() -> String {
    default_workspace_path().add(&format!("/{}", default_units_folder()))
}

fn default_modules_folder() -> String {
    "modules".into()
}
fn default_modules_path() -> String {
    default_workspace_path().add(&format!("/{}", default_modules_folder()))
}

fn default_plugins_folder() -> String {
    "plugins".into()
}
fn default_plugins_path() -> String {
    default_workspace_path().add(&format!("/{}", default_plugins_folder()))
}

fn default_inventory_folder() -> String {
    "inventory".into()
}
fn default_inventory_path() -> String {
    default_workspace_path().add(&format!("/{}", default_inventory_folder()))
}

fn default_dim_relations() -> Vec<String> {
    ["dome".into(), "env".into(), "dc".into()].into()
}
fn default_orgs() -> Vec<String> {
    ["cubtera".into()].into()
}
fn default_org() -> String {
    "cubtera".into()
}
fn default_file_name_separator() -> String {
    ":".into()
}

impl Default for CubteraConfig {
    fn default() -> Self {
        Self {
            workspace_path: default_workspace_path(),
            inventory_path: default_inventory_path(),
            units_path: default_units_path(),
            modules_path: default_modules_path(),
            plugins_path: default_plugins_path(),
            temp_folder_path: default_temp_folder_path(),
            org: default_org(),
            orgs: default_orgs(),
            dim_relations: default_dim_relations(),
            db: None,
            dlog_db: None,
            dlog_job_user_name_env: None,
            dlog_job_number_env: None,
            dlog_job_name_env: None,
            clean_cache: false,
            always_copy_files: true,
            runner: None,
            state: None,
            db_client: None,
            file_name_separator: default_file_name_separator(),
        }
    }
}

impl CubteraConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

impl CubteraConfig {
    pub fn build(self) -> Self {
        // read all CUBTERA_* env vars overrides

        let cfg_env_vars = config::Config::builder()
            .add_source(
                config::Environment::with_prefix("CUBTERA")
                    .ignore_empty(true)
                    .prefix_separator("_")
                    .separator("__"), //.try_parsing(true)
            )
            .build()
            .unwrap_or_exit("Failed to read env vars".to_string())
            .try_deserialize::<HashMap<String, Value>>()
            .unwrap_or_exit("Failed to deserialize env vars".to_string());

        //
        // let cfg_env_vars = config::Config::builder()
        //     .add_source(
        //         config::Environment::with_prefix("CUBTERA")
        //             .ignore_empty(true)
        //             //.prefix_separator("_")
        //             // .separator("__"),
        //             // .try_parsing(true)
        //             // .list_separator(",")
        //             // .with_list_parse_key("DB")
        //     )
        //     .build()
        //     .unwrap_or_exit("Failed to read env vars".to_string())
        //     .try_deserialize::<HashMap<String, String>>()
        //     .unwrap_or_exit("Failed to deserialize env vars".to_string());

        // read config file path from env vars
        let cfg_file_path = cfg_env_vars
            .get("config")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .map(|s| convert_path_to_absolute(s.clone()).unwrap_or(s.clone()))
            .unwrap_or_else(|| {
                let path = "~/.cubtera/config.toml";
                warn!(target: "cfg", "CUBTERA_CONFIG is not provided. Using: {}", path.blue());
                
                convert_path_to_absolute(path.to_string()).unwrap_or_default()
            });

        // Read settings from config file (TOML) if it exists and provided in env vars
        let cfg_file = config::Config::builder()
            .add_source(config::File::from(Path::new(&cfg_file_path)))
            .build()
            .and_then(|cfg| cfg.try_deserialize::<HashMap<String, HashMap<String, Value>>>())
            .check_with_warn("Failed to read config file")
            .unwrap_or_default();

        // .unwrap_or_else(|e|{
        //     debug!("Using default config. Failed to read config file {}: {}", cfg_file_path, e);
        //     config::Config::default()
        // })
        // .try_deserialize::<HashMap<String, HashMap<String, Value>>>()
        // .unwrap_or_exit("Failed to deserialize config file".to_string());

        // read default section from config file
        let mut config = cfg_file.get("default").cloned().unwrap_or_default();
        let org = cfg_env_vars
            .get("org")
            .cloned()
            .map(|v| v.as_str().unwrap().to_string())
            .unwrap_or_else(|| {
                warn!(target: "cfg", "CUBTERA_ORG is not provided. Using: {}", "cubtera".blue());
                "cubtera".to_string()
            });

        // extend with override default section with org section from config file
        config.extend(cfg_file.get(&org).cloned().unwrap_or_default());
        // extend with override default and org sections with env vars values
        #[allow(clippy::map_identity)]
        config.extend(
            cfg_env_vars
                .into_iter()
                .map(|(key, value)| (key, value))
                .collect::<HashMap<String, Value>>(),
        );

        if config.is_empty() {
            warn!(target: "cfg", "Config is not provided. Using default values");
            return self;
        }
        // deserialize config to CubteraConfig struct
        let config_value = serde_json::to_value(&config).unwrap();

        let mut cfg = serde_json::from_value::<CubteraConfig>(config_value)
            .unwrap_or_exit("Failed to deserialize config".to_string());

        // set paths if not provided in config file or env vars
        cfg.modules_path = cfg.define_path(config.clone(), "modules");
        cfg.units_path = cfg.define_path(config.clone(), "units");
        cfg.plugins_path = cfg.define_path(config.clone(), "plugins");
        cfg.inventory_path = cfg.define_path(config.clone(), "inventory");

        if let Some(db) = cfg.db.clone() {
            cfg.db_client = Some(db_connect(&db));
        }

        cfg
    }

    fn define_path(&self, config: HashMap<String, Value>, path_type: &str) -> String {
        config
            .get(&format!("{}_path", path_type))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(|| {
                self.workspace_path.clone().add("/").add(
                    &config
                        .contains_key(&format!("{}_folder", path_type))
                        .then(|| format!("{}/", path_type))
                        .unwrap_or_else(|| match path_type {
                            "plugins" => default_plugins_folder(),
                            "modules" => default_modules_folder(),
                            "units" => default_units_folder(),
                            "inventory" => default_inventory_folder(),
                            _ => "".into(),
                        }),
                )
            })
    }

    pub fn get_values(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub fn get_db(&self) -> Option<mongodb::sync::Client> {
        self.db_client.clone()
    }

    pub fn get_runner_by_type(&self, runner_type: &str) -> Option<HashMap<String, String>> {
        self.runner
            .clone()
            .and_then(|r| r.get(runner_type).cloned())
    }

    pub fn get_json(&self) -> String {
        serde_json::to_string_pretty(&self).unwrap_or_default()
    }

    pub fn get_toml(&self) -> String {
        toml::to_string_pretty(&self).unwrap_or_default()
    }
}

use serde::de::Deserializer;
use yansi::Paint;

fn deserialize_colon_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let s: String = String::deserialize(deserializer)?;
    Ok(s.split(':').map(|s| s.to_string()).collect())
}

use serde::Serializer;

#[allow(clippy::ptr_arg)]
fn serialize_colon_list<S>(list: &Vec<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let joined = list.join(":");
    serializer.serialize_str(&joined)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::collections::HashMap;

    #[test]
    fn test_default_config_values() {
        let config = CubteraConfig::default();

        assert_eq!(config.workspace_path, "~/.cubtera/workspace");
        assert_eq!(config.temp_folder_path, "~/.cubtera/tmp");
        assert_eq!(config.inventory_path, "~/.cubtera/workspace/inventory");
        assert_eq!(config.units_path, "~/.cubtera/workspace/units");
        assert_eq!(config.modules_path, "~/.cubtera/workspace/modules");
        assert_eq!(config.plugins_path, "~/.cubtera/workspace/plugins");
        assert_eq!(config.org, "cubtera");
        assert_eq!(config.orgs, vec!["cubtera"]);
        assert_eq!(config.dim_relations, vec!["dome", "env", "dc"]);
        assert_eq!(config.file_name_separator, ":");
        assert!(!config.clean_cache);
        assert!(config.always_copy_files);
        assert!(config.db.is_none());
        assert!(config.dlog_db.is_none());
        assert!(config.runner.is_none());
        assert!(config.state.is_none());
        assert!(config.db_client.is_none());
    }

    #[test]
    fn test_new_equals_default() {
        let config_new = CubteraConfig::new();
        let config_default = CubteraConfig::default();

        assert_eq!(config_new.workspace_path, config_default.workspace_path);
        assert_eq!(config_new.org, config_default.org);
        assert_eq!(config_new.orgs, config_default.orgs);
        assert_eq!(config_new.dim_relations, config_default.dim_relations);
    }

    #[test]
    fn test_default_path_functions() {
        assert_eq!(default_workspace_path(), "~/.cubtera/workspace");
        assert_eq!(default_temp_folder_path(), "~/.cubtera/tmp");
        assert_eq!(default_units_folder(), "units");
        assert_eq!(default_modules_folder(), "modules");
        assert_eq!(default_plugins_folder(), "plugins");
        assert_eq!(default_inventory_folder(), "inventory");
        assert_eq!(default_org(), "cubtera");
        assert_eq!(default_file_name_separator(), ":");
    }

    #[test]
    fn test_default_dim_relations() {
        let relations = default_dim_relations();
        assert_eq!(relations.len(), 3);
        assert_eq!(relations[0], "dome");
        assert_eq!(relations[1], "env");
        assert_eq!(relations[2], "dc");
    }

    #[test]
    fn test_default_orgs() {
        let orgs = default_orgs();
        assert_eq!(orgs.len(), 1);
        assert_eq!(orgs[0], "cubtera");
    }

    #[test]
    fn test_define_path_with_explicit_path() {
        let config = CubteraConfig::default();
        let mut hashmap: HashMap<String, Value> = HashMap::new();
        hashmap.insert("units_path".to_string(), json!("/custom/units/path"));

        let result = config.define_path(hashmap, "units");
        assert_eq!(result, "/custom/units/path");
    }

    #[test]
    fn test_define_path_with_folder_override() {
        let config = CubteraConfig::default();
        let mut hashmap: HashMap<String, Value> = HashMap::new();
        hashmap.insert("units_folder".to_string(), json!("custom_units"));

        let result = config.define_path(hashmap, "units");
        // When folder is specified, it uses path_type/ format
        assert!(result.contains("units/"));
    }

    #[test]
    fn test_define_path_defaults() {
        let config = CubteraConfig::default();
        let hashmap: HashMap<String, Value> = HashMap::new();

        let units_path = config.define_path(hashmap.clone(), "units");
        let modules_path = config.define_path(hashmap.clone(), "modules");
        let plugins_path = config.define_path(hashmap.clone(), "plugins");
        let inventory_path = config.define_path(hashmap.clone(), "inventory");

        assert!(units_path.ends_with("units"));
        assert!(modules_path.ends_with("modules"));
        assert!(plugins_path.ends_with("plugins"));
        assert!(inventory_path.ends_with("inventory"));
    }

    #[test]
    fn test_get_runner_by_type_none() {
        let config = CubteraConfig::default();
        let result = config.get_runner_by_type("tf");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_runner_by_type_exists() {
        let mut config = CubteraConfig::default();
        let mut tf_runner: HashMap<String, String> = HashMap::new();
        tf_runner.insert("version".to_string(), "1.5.0".to_string());
        tf_runner.insert("state_backend".to_string(), "s3".to_string());

        let mut runners: HashMap<String, HashMap<String, String>> = HashMap::new();
        runners.insert("tf".to_string(), tf_runner);
        config.runner = Some(runners);

        let result = config.get_runner_by_type("tf");
        assert!(result.is_some());
        let runner = result.unwrap();
        assert_eq!(runner.get("version"), Some(&"1.5.0".to_string()));
        assert_eq!(runner.get("state_backend"), Some(&"s3".to_string()));
    }

    #[test]
    fn test_get_runner_by_type_wrong_type() {
        let mut config = CubteraConfig::default();
        let mut tf_runner: HashMap<String, String> = HashMap::new();
        tf_runner.insert("version".to_string(), "1.5.0".to_string());

        let mut runners: HashMap<String, HashMap<String, String>> = HashMap::new();
        runners.insert("tf".to_string(), tf_runner);
        config.runner = Some(runners);

        let result = config.get_runner_by_type("bash");
        assert!(result.is_none());
    }

    #[test]
    fn test_get_values() {
        let config = CubteraConfig::default();
        let result = config.get_values();

        assert!(result.is_ok());
        let value = result.unwrap();
        assert!(value.is_object());
        assert_eq!(value["org"], "cubtera");
        assert_eq!(value["workspace_path"], "~/.cubtera/workspace");
    }

    #[test]
    fn test_get_json() {
        let config = CubteraConfig::default();
        let json_str = config.get_json();

        assert!(!json_str.is_empty());
        assert!(json_str.contains("cubtera"));
        assert!(json_str.contains("workspace_path"));

        // Verify it's valid JSON
        let parsed: Result<Value, _> = serde_json::from_str(&json_str);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_get_toml() {
        let config = CubteraConfig::default();
        let toml_str = config.get_toml();

        assert!(!toml_str.is_empty());
        assert!(toml_str.contains("cubtera"));
        assert!(toml_str.contains("workspace_path"));
    }

    #[test]
    fn test_get_db_none() {
        let config = CubteraConfig::default();
        let result = config.get_db();
        assert!(result.is_none());
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = CubteraConfig::default();

        // Serialize to JSON
        let json_str = serde_json::to_string(&config).unwrap();

        // Deserialize back
        let deserialized: CubteraConfig = serde_json::from_str(&json_str).unwrap();

        assert_eq!(config.workspace_path, deserialized.workspace_path);
        assert_eq!(config.org, deserialized.org);
        assert_eq!(config.orgs, deserialized.orgs);
        assert_eq!(config.dim_relations, deserialized.dim_relations);
    }

    #[test]
    fn test_colon_list_serialization() {
        let config = CubteraConfig::default();
        let toml_str = config.get_toml();

        // orgs and dim_relations should be serialized as colon-separated strings
        assert!(toml_str.contains("orgs = \"cubtera\"") || toml_str.contains("orgs = 'cubtera'"));
        assert!(
            toml_str.contains("dim_relations = \"dome:env:dc\"")
                || toml_str.contains("dim_relations = 'dome:env:dc'")
        );
    }

    #[test]
    fn test_config_with_optional_fields() {
        let mut config = CubteraConfig::default();
        config.db = Some("mongodb://localhost:27017".to_string());
        config.dlog_db = Some("mongodb://localhost:27017/dlog".to_string());
        config.dlog_job_user_name_env = Some("USER".to_string());
        config.dlog_job_number_env = Some("BUILD_NUMBER".to_string());
        config.dlog_job_name_env = Some("JOB_NAME".to_string());

        let json_str = config.get_json();
        assert!(json_str.contains("mongodb://localhost:27017"));
        assert!(json_str.contains("USER"));
        assert!(json_str.contains("BUILD_NUMBER"));
        assert!(json_str.contains("JOB_NAME"));
    }

    #[test]
    fn test_config_with_state_backend() {
        let mut config = CubteraConfig::default();

        let mut s3_config: HashMap<String, String> = HashMap::new();
        s3_config.insert("bucket".to_string(), "my-bucket".to_string());
        s3_config.insert("region".to_string(), "us-east-1".to_string());
        s3_config.insert("key".to_string(), "{{ dim_tree }}/{{ unit_name }}.tfstate".to_string());

        let mut state: HashMap<String, HashMap<String, String>> = HashMap::new();
        state.insert("s3".to_string(), s3_config);
        config.state = Some(state);

        let json_str = config.get_json();
        assert!(json_str.contains("my-bucket"));
        assert!(json_str.contains("us-east-1"));
    }

    #[test]
    fn test_deserialize_from_json_with_colon_lists() {
        let json = json!({
            "workspace_path": "/custom/workspace",
            "org": "myorg",
            "orgs": "org1:org2:org3",
            "dim_relations": "a:b:c:d"
        });

        let config: CubteraConfig = serde_json::from_value(json).unwrap();

        assert_eq!(config.workspace_path, "/custom/workspace");
        assert_eq!(config.org, "myorg");
        assert_eq!(config.orgs, vec!["org1", "org2", "org3"]);
        assert_eq!(config.dim_relations, vec!["a", "b", "c", "d"]);
    }
}
