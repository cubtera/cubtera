use crate::utils::helper::*;
use crate::prelude::*;
use std::collections::HashMap;
use std::ops::Add;
use std::path::Path;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use once_cell::sync::Lazy;


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
    #[serde(default = "default_tf_state_s3bucket")]
    pub tf_state_s3bucket: String,
    #[serde(default = "default_tf_state_s3region")]
    pub tf_state_s3region: String,
    #[serde(deserialize_with = "deserialize_colon_list", serialize_with="serialize_colon_list", default="default_orgs")]
    pub orgs: Vec<String>,
    #[serde(deserialize_with = "deserialize_colon_list", serialize_with="serialize_colon_list", default="default_dim_relations")]
    pub dim_relations: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tf_state_key_prefix: Option<String>,
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
}

fn default_workspace_path() -> String { "~/.cubtera/workspace".into() }
fn default_temp_folder_path() -> String { "~/.cubtera/tmp".into() }

fn default_units_folder() -> String { "units".into() }
fn default_units_path() -> String { default_workspace_path()
    .add(&format!("/{}",default_units_folder()))
}

fn default_modules_folder() -> String { "modules".into() }
fn default_modules_path() -> String { default_workspace_path()
    .add(&format!("/{}",default_modules_folder()))
}

fn default_plugins_folder() -> String { "plugins".into() }
fn default_plugins_path() -> String { default_workspace_path()
    .add(&format!("/{}",default_plugins_folder()))
}

fn default_inventory_folder() -> String { "inventory".into() }
fn default_inventory_path() -> String { default_workspace_path()
    .add(&format!("/{}",default_inventory_folder()))
}

fn default_dim_relations() -> Vec<String> { ["dome".into(), "env".into(), "dc".into()].into() }
fn default_orgs() -> Vec<String> { ["cubtera".into()].into() }
fn default_org() -> String { "cubtera".into() }

fn default_tf_state_s3bucket() -> String { "cubtera_state_bucket".to_string() }
fn default_tf_state_s3region() -> String { "us-east-1".to_string() }

impl Default for CubteraConfig {
    fn default() -> Self {
        Self {
            workspace_path: default_workspace_path(),
            inventory_path: default_inventory_path(),
            units_path: default_units_path(),
            modules_path: default_modules_path(),
            plugins_path: default_plugins_path(),
            temp_folder_path: default_temp_folder_path(),

            tf_state_s3bucket: default_tf_state_s3bucket(),
            tf_state_s3region: default_tf_state_s3region(),
            tf_state_key_prefix: None,

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
        }
    }
}

impl CubteraConfig {
    pub fn new() -> Self {
        Self::default()
    }
}

#[derive(Debug, Deserialize)]
struct EnvConfig {
    regular: HashMap<String, String>,
    nested: HashMap<String, HashMap<String, String>>,
    // Add other fields as needed
}

impl CubteraConfig {
    pub fn build(self) -> Self {
        // read all CUBTERA_* env vars overrides

        let cfg_env_vars = config::Config::builder()
            .add_source(config::Environment::with_prefix("CUBTERA")
                .ignore_empty(true)
                .prefix_separator("_")
                .separator("__")
                //.try_parsing(true)
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
            // .cloned()
            .unwrap_or_else(||{
                let path = "~/.cubtera/config.toml";
                warn!(target: "cfg", "CUBTERA_CONFIG is not provided. Using: {}", path.blue());
                let full_path = convert_path_to_absolute(path.to_string())
                    .unwrap_or_default();
                    // path.replace('~', &std::env::var("HOME").unwrap_or_default());

                // if !Path::new(&full_path).exists() {
                //     warn!(target: "cfg", "Create default config file: {}", path.blue());
                //
                //     let value = self.get_values().unwrap_or_default();
                //     let value = serde_json::json!({ "default": value });
                //     let toml_str = toml::to_string_pretty(&value).unwrap();
                //
                //     std::fs::create_dir_all(Path::new(&full_path).parent().unwrap())
                //         .unwrap_or_exit(format!("Failed to create folders for file {}", path));
                //     std::fs::write(&full_path, toml_str)
                //         .unwrap_or_exit(format!("Failed to write config file {}", path));
                // }

                full_path
            });



        // Read settings from config file (TOML) if it exists and provided in env vars
        let cfg_file = config::Config::builder()
            .add_source(config::File::from(Path::new(&cfg_file_path)))
            .build()
            .and_then(| cfg | cfg.try_deserialize::<HashMap<String, HashMap<String, Value>>>())
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
        let org = cfg_env_vars.get("org").cloned()
            .map(|v| v.as_str().unwrap().to_string())
            .unwrap_or_else(||{
            warn!(target: "cfg", "CUBTERA_ORG is not provided. Using: {}", "cubtera".blue());
            "cubtera".to_string()
        });

        // extend with override default section with org section from config file
        config.extend(cfg_file.get(&org).cloned().unwrap_or_default());
        // extend with override default and org sections with env vars values
        config.extend(cfg_env_vars.into_iter()
            .map(|(key, value)| (key, value))
            .collect::<HashMap<String, Value>>());

        if config.is_empty() {
            warn!(target: "cfg", "Config is not provided. Using default values");
            return self
        }
        // deserialize config to CubteraConfig struct
        let config_value = serde_json::to_value(&config).unwrap();

        let mut cfg = serde_json::from_value::<CubteraConfig>(config_value).unwrap_or_exit("Failed to deserialize config".to_string());

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
        config.get(&format!("{}_path", path_type))
            .and_then(|v| v.as_str().map(|s| s.to_string()))
            .unwrap_or_else(||{
                self.workspace_path.clone().add("/").add(
                    &config.contains_key(&format!("{}_folder", path_type))
                        .then(|| format!("{}/", path_type))
                        .unwrap_or_else(|| {
                            match path_type {
                                "plugins" => default_plugins_folder(),
                                "modules" => default_modules_folder(),
                                "units" => default_units_folder(),
                                "inventory" => default_inventory_folder(),
                                _ => "".into(),
                            }
                        })
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
        self.runner.clone().and_then(|r| r.get(runner_type).cloned())
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

fn serialize_colon_list<S>(list: &Vec<String>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let joined = list.join(":");
    serializer.serialize_str(&joined)
}