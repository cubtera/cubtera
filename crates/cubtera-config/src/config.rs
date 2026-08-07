//! Configuration types and loading
//!
//! Schema: a `[default]` table plus one arbitrary table per org name
//! (`[cubtera]`, `[teracub]`, ...) whose fields override `[default]`'s for
//! that org - the same shape v1's `config.toml` used. Loading itself goes
//! through [`ConfigProvider`] + [`ConfigSource`] so the merge/override logic
//! is testable without touching the real filesystem or process environment;
//! [`Config::load`]/[`Config::load_from_path`] are just the real-world
//! convenience entry points built on top of it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration errors
#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read config file: {0}")]
    ReadFile(#[from] std::io::Error),

    #[error("Failed to parse config: {0}")]
    Parse(String),

    #[error("Missing required configuration: {0}")]
    Missing(String),
}

/// Where config bytes and environment overrides come from. Injectable so
/// [`ConfigProvider::load`] never has to touch the real filesystem or
/// process environment in tests.
pub trait ConfigSource {
    /// Raw TOML content, or `None` if there's nothing to read (falls back
    /// to built-in defaults - a missing config file is not an error).
    fn file_contents(&self) -> Option<String>;
    /// Read a single environment-style override variable.
    fn env(&self, key: &str) -> Option<String>;
}

/// Real-world [`ConfigSource`]: a file on disk plus `std::env`.
pub struct FsConfigSource {
    pub path: PathBuf,
}

impl ConfigSource for FsConfigSource {
    fn file_contents(&self) -> Option<String> {
        fs::read_to_string(&self.path).ok()
    }

    fn env(&self, key: &str) -> Option<String> {
        env::var(key).ok()
    }
}

/// In-memory [`ConfigSource`] for tests: no filesystem, no process
/// environment.
#[derive(Debug, Clone, Default)]
pub struct StaticConfigSource {
    pub file_contents: Option<String>,
    pub env: HashMap<String, String>,
}

impl ConfigSource for StaticConfigSource {
    fn file_contents(&self) -> Option<String> {
        self.file_contents.clone()
    }

    fn env(&self, key: &str) -> Option<String> {
        self.env.get(key).cloned()
    }
}

/// Loads and merges [`Config`] from an injected [`ConfigSource`].
pub struct ConfigProvider;

impl ConfigProvider {
    /// Parse the source's TOML (if any), pick the active org (`CUBTERA_ORG`
    /// env override, else the first entry in `[default].orgs`, else
    /// `"default"`), merge `[default]` with that org's table (org wins), and
    /// apply any remaining environment overrides.
    pub fn load(source: &dyn ConfigSource) -> Result<Config, ConfigError> {
        let raw: RawConfigFile = match source.file_contents() {
            Some(content) => {
                toml::from_str(&content).map_err(|e| ConfigError::Parse(e.to_string()))?
            }
            None => RawConfigFile::default(),
        };

        let org = source
            .env("CUBTERA_ORG")
            .or_else(|| raw.default.orgs.as_ref().and_then(|o| o.first().cloned()))
            .unwrap_or_else(|| "default".to_string());

        let org_override = raw.orgs.get(&org).cloned().unwrap_or_default();
        let merged = PartialConfig::merge(&raw.default, &org_override);

        let mut config = merged.into_config(org);
        config.apply_env_overrides(source);
        Ok(config)
    }
}

/// The raw shape of `config.toml`: a `[default]` table plus arbitrary
/// per-org tables, keyed by org name.
#[derive(Debug, Clone, Default, Deserialize)]
struct RawConfigFile {
    #[serde(default)]
    default: PartialConfig,
    #[serde(flatten)]
    orgs: HashMap<String, PartialConfig>,
}

/// Every [`Config`] field, optional, so an org table only has to specify
/// what it overrides. `runner`/`state` merge key-by-key (an org can override
/// just `[cubtera.state.s3]` without losing `[default.state.local]`); every
/// other field is a full override.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PartialConfig {
    #[serde(alias = "inventory_path")]
    inventory_path: Option<PathBuf>,
    #[serde(alias = "units_path")]
    units_path: Option<PathBuf>,
    #[serde(alias = "modules_path")]
    modules_path: Option<PathBuf>,
    #[serde(alias = "plugins_path")]
    plugins_path: Option<PathBuf>,
    #[serde(alias = "temp_folder_path")]
    temp_folder_path: Option<PathBuf>,
    /// FS-jsonl deployment log root, used when `deployment_log` (Mongo) is
    /// unset.
    #[serde(alias = "deployment_log_path")]
    deployment_log_path: Option<PathBuf>,
    #[serde(alias = "dim_relations")]
    dim_relations: Option<Vec<String>>,
    orgs: Option<Vec<String>>,
    #[serde(alias = "file_name_separator")]
    file_name_separator: Option<String>,
    #[serde(alias = "always_copy_files")]
    always_copy_files: Option<bool>,
    #[serde(alias = "clean_cache")]
    clean_cache: Option<bool>,
    #[serde(alias = "log_level")]
    log_level: Option<String>,
    #[serde(default)]
    runner: HashMap<String, HashMap<String, String>>,
    #[serde(default)]
    state: HashMap<String, StateBackendConfig>,
    #[serde(alias = "deployment_log")]
    deployment_log: Option<DeploymentLogConfig>,
    #[serde(alias = "api_key")]
    api_key: Option<String>,
}

impl PartialConfig {
    /// Merge `over` on top of `base`: `over`'s `Some` values win field by
    /// field; `runner`/`state` maps merge entry-by-entry instead of
    /// replacing wholesale.
    fn merge(base: &PartialConfig, over: &PartialConfig) -> PartialConfig {
        PartialConfig {
            inventory_path: over
                .inventory_path
                .clone()
                .or_else(|| base.inventory_path.clone()),
            units_path: over.units_path.clone().or_else(|| base.units_path.clone()),
            modules_path: over
                .modules_path
                .clone()
                .or_else(|| base.modules_path.clone()),
            plugins_path: over
                .plugins_path
                .clone()
                .or_else(|| base.plugins_path.clone()),
            temp_folder_path: over
                .temp_folder_path
                .clone()
                .or_else(|| base.temp_folder_path.clone()),
            deployment_log_path: over
                .deployment_log_path
                .clone()
                .or_else(|| base.deployment_log_path.clone()),
            dim_relations: over
                .dim_relations
                .clone()
                .or_else(|| base.dim_relations.clone()),
            orgs: over.orgs.clone().or_else(|| base.orgs.clone()),
            file_name_separator: over
                .file_name_separator
                .clone()
                .or_else(|| base.file_name_separator.clone()),
            always_copy_files: over.always_copy_files.or(base.always_copy_files),
            clean_cache: over.clean_cache.or(base.clean_cache),
            log_level: over.log_level.clone().or_else(|| base.log_level.clone()),
            runner: merge_entries(&base.runner, &over.runner),
            state: merge_entries(&base.state, &over.state),
            deployment_log: over
                .deployment_log
                .clone()
                .or_else(|| base.deployment_log.clone()),
            api_key: over.api_key.clone().or_else(|| base.api_key.clone()),
        }
    }

    fn into_config(self, org: String) -> Config {
        Config {
            org,
            orgs: self.orgs.unwrap_or_default(),
            inventory_path: self.inventory_path.unwrap_or_else(default_inventory_path),
            units_path: self.units_path.unwrap_or_else(default_units_path),
            modules_path: self.modules_path.unwrap_or_else(default_modules_path),
            plugins_path: self.plugins_path.unwrap_or_else(default_plugins_path),
            temp_folder_path: self
                .temp_folder_path
                .unwrap_or_else(default_temp_folder_path),
            deployment_log_path: self
                .deployment_log_path
                .unwrap_or_else(default_deployment_log_path),
            dim_relations: self.dim_relations.unwrap_or_else(default_dim_relations),
            file_name_separator: self
                .file_name_separator
                .unwrap_or_else(default_file_name_separator),
            always_copy_files: self.always_copy_files.unwrap_or(false),
            clean_cache: self.clean_cache.unwrap_or(false),
            log_level: self.log_level.unwrap_or_else(default_log_level),
            runner: self.runner,
            state: self.state,
            deployment_log: self.deployment_log,
            mongodb_connection_string: None,
            api_key: self.api_key,
        }
    }
}

/// Merge two maps entry-by-entry, `over`'s entries winning on key collision.
fn merge_entries<V: Clone>(
    base: &HashMap<String, V>,
    over: &HashMap<String, V>,
) -> HashMap<String, V> {
    let mut merged = base.clone();
    for (k, v) in over {
        merged.insert(k.clone(), v.clone());
    }
    merged
}

/// Fully-resolved configuration for one org - everything downstream
/// (repositories, runners, CLI) needs is a concrete value, no further
/// merging or environment lookups.
#[derive(Debug, Clone, Serialize)]
pub struct Config {
    /// Active organization name
    pub org: String,
    /// All known organizations (for `im`/listing commands)
    pub orgs: Vec<String>,

    /// Path to the FS inventory directory
    pub inventory_path: PathBuf,
    /// Path to units directory
    pub units_path: PathBuf,
    /// Path to modules directory
    pub modules_path: PathBuf,
    /// Path to plugins directory
    pub plugins_path: PathBuf,
    /// Path to temp folder for runner execution
    pub temp_folder_path: PathBuf,
    /// Root directory for the FS-jsonl deployment log backend (one
    /// `{org}.jsonl` file per org), used unless `deployment_log` (Mongo) is
    /// set
    pub deployment_log_path: PathBuf,

    /// Dimension relations (hierarchy)
    pub dim_relations: Vec<String>,
    /// Separator between a dimension/unit name and its section/include
    /// suffix in inventory file names (e.g. `admin:manifest.json`)
    pub file_name_separator: String,

    /// Always copy files before run (not just on init)
    pub always_copy_files: bool,
    /// Clean temp cache after successful apply/destroy
    pub clean_cache: bool,

    /// Per-runner-type config, e.g. `runner["tf"]["state_backend"] = "s3"`
    pub runner: HashMap<String, HashMap<String, String>>,
    /// Per-backend state config template, e.g. `state["s3"]`; values may
    /// contain handlebars placeholders (`{{org}}`, `{{unit_name}}`,
    /// `{{dim_tree}}`) rendered at run time
    /// (see [`cubtera_domain::render_state_backend_config`])
    pub state: HashMap<String, StateBackendConfig>,

    /// Deployment log configuration
    pub deployment_log: Option<DeploymentLogConfig>,

    /// Log level
    pub log_level: String,

    /// MongoDB connection string (`CUBTERA_DB` env var only - wave 2, not
    /// yet a `config.toml` field: there's no real Mongo adapter behind it)
    pub mongodb_connection_string: Option<String>,

    /// API key `cubtera-api` requires on the `x-api-key` header for `/v1/*`
    /// routes. `None` means auth is disabled (local dev default). Prefer
    /// `CUBTERA_API_KEY` over the `apiKey` config field so the secret
    /// doesn't have to live in `config.toml`.
    #[serde(skip)]
    pub api_key: Option<String>,
}

fn default_inventory_path() -> PathBuf {
    PathBuf::from("inventory")
}

fn default_units_path() -> PathBuf {
    PathBuf::from("units")
}

fn default_modules_path() -> PathBuf {
    PathBuf::from("modules")
}

fn default_plugins_path() -> PathBuf {
    PathBuf::from("plugins")
}

fn default_temp_folder_path() -> PathBuf {
    home_dir()
        .map(|h| h.join(".cubtera").join("temp"))
        .unwrap_or_else(|| PathBuf::from("/tmp/cubtera"))
}

fn default_deployment_log_path() -> PathBuf {
    home_dir()
        .map(|h| h.join(".cubtera").join("dlog"))
        .unwrap_or_else(|| PathBuf::from("/tmp/cubtera-dlog"))
}

fn default_dim_relations() -> Vec<String> {
    vec!["dome".to_string(), "env".to_string(), "dc".to_string()]
}

fn default_file_name_separator() -> String {
    ":".to_string()
}

fn default_log_level() -> String {
    "info".to_string()
}

fn home_dir() -> Option<PathBuf> {
    env::var("HOME")
        .or_else(|_| env::var("USERPROFILE"))
        .ok()
        .map(PathBuf::from)
}

impl Default for Config {
    fn default() -> Self {
        PartialConfig::default().into_config("default".to_string())
    }
}

impl Config {
    /// Load configuration from `$CUBTERA_CONFIG` (or `~/.cubtera/config.toml`)
    /// and the process environment.
    pub fn load() -> Result<Self, ConfigError> {
        let config_path = env::var("CUBTERA_CONFIG")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                home_dir()
                    .map(|h| h.join(".cubtera").join("config.toml"))
                    .unwrap_or_else(|| PathBuf::from("config.toml"))
            });

        Self::load_from_path(&config_path)
    }

    /// Load configuration from a specific path and the process environment.
    pub fn load_from_path(path: &Path) -> Result<Self, ConfigError> {
        ConfigProvider::load(&FsConfigSource {
            path: path.to_path_buf(),
        })
    }

    /// Apply environment variable overrides on top of an already-merged config
    fn apply_env_overrides(&mut self, source: &dyn ConfigSource) {
        if let Some(org) = source.env("CUBTERA_ORG") {
            self.org = org;
        }

        if let Some(log_level) = source.env("CUBTERA_LOG") {
            self.log_level = log_level;
        }

        if let Some(db_url) = source.env("CUBTERA_DB") {
            self.mongodb_connection_string = Some(db_url);
        }

        if let Some(api_key) = source.env("CUBTERA_API_KEY") {
            self.api_key = Some(api_key);
        }

        if let Some(inventory_path) = source.env("CUBTERA_INVENTORY_PATH") {
            self.inventory_path = PathBuf::from(inventory_path);
        }

        if let Some(units_path) = source.env("CUBTERA_UNITS_PATH") {
            self.units_path = PathBuf::from(units_path);
        }

        if let Some(modules_path) = source.env("CUBTERA_MODULES_PATH") {
            self.modules_path = PathBuf::from(modules_path);
        }

        if let Some(temp_path) = source.env("CUBTERA_TEMP_PATH") {
            self.temp_folder_path = PathBuf::from(temp_path);
        }

        if let Some(dlog_path) = source.env("CUBTERA_DLOG_PATH") {
            self.deployment_log_path = PathBuf::from(dlog_path);
        }

        if source.env("CUBTERA_ALWAYS_COPY_FILES").is_some() {
            self.always_copy_files = true;
        }

        if source.env("CUBTERA_CLEAN_CACHE").is_some() {
            self.clean_cache = true;
        }
    }
}

/// State backend configuration template
#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct StateBackendConfig {
    /// Backend-specific options (supports handlebars templates)
    #[serde(flatten)]
    pub options: HashMap<String, serde_json::Value>,
}

/// Deployment log configuration - selects [`crate::config::MongoDeploymentLogRepository`]-style
/// Mongo backend for the deployment log port; unset means fs-jsonl
/// (`Config::deployment_log_path`).
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentLogConfig {
    /// MongoDB connection string for deployment logs
    #[serde(alias = "connection_string")]
    pub connection_string: String,
    /// Database name - shared by every org, which is distinguished by each
    /// entry's own `org` field
    #[serde(default = "default_dlog_database")]
    pub database: String,
    /// Collection name
    #[serde(default = "default_dlog_collection")]
    pub collection: String,
}

fn default_dlog_database() -> String {
    "cubtera".to_string()
}

fn default_dlog_collection() -> String {
    "deployments".to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.org, "default");
        assert_eq!(config.inventory_path, PathBuf::from("inventory"));
    }

    #[test]
    fn test_config_inventory_path() {
        let config = Config {
            inventory_path: PathBuf::from("/tmp/inv"),
            ..Config::default()
        };
        assert_eq!(config.inventory_path, PathBuf::from("/tmp/inv"));
    }

    fn source(toml: &str) -> StaticConfigSource {
        StaticConfigSource {
            file_contents: Some(toml.to_string()),
            env: HashMap::new(),
        }
    }

    #[test]
    fn load_falls_back_to_defaults_with_no_file() {
        let config = ConfigProvider::load(&StaticConfigSource::default()).unwrap();
        assert_eq!(config.org, "default");
        assert_eq!(config.inventory_path, PathBuf::from("inventory"));
    }

    #[test]
    fn load_picks_first_org_when_no_env_override() {
        let config = ConfigProvider::load(&source(
            r#"
            [default]
            orgs = ["cubtera", "teracub"]
            "#,
        ))
        .unwrap();
        assert_eq!(config.org, "cubtera");
        assert_eq!(config.orgs, vec!["cubtera", "teracub"]);
    }

    #[test]
    fn load_merges_org_table_over_default_table() {
        let config = ConfigProvider::load(&source(
            r#"
            [default]
            orgs = ["cubtera"]
            inventoryPath = "inventory"
            alwaysCopyFiles = true

            [cubtera]
            inventoryPath = "example/inventory"
            "#,
        ))
        .unwrap();

        assert_eq!(config.inventory_path, PathBuf::from("example/inventory"));
        // Not overridden by [cubtera], so it falls through from [default].
        assert!(config.always_copy_files);
    }

    #[test]
    fn load_merges_state_and_runner_maps_entry_by_entry() {
        let config = ConfigProvider::load(&source(
            r#"
            [default]
            orgs = ["cubtera"]

            [default.state.local]
            path = "~/.cubtera/state/{{org}}/{{dim_tree}}/{{unit_name}}.tfstate"

            [cubtera.state.s3]
            bucket = "{{org}}-example-state"
            "#,
        ))
        .unwrap();

        assert!(
            config.state.contains_key("local"),
            "default-only entries survive the merge"
        );
        assert!(
            config.state.contains_key("s3"),
            "org-only entries are added"
        );
        assert_eq!(
            config.state["s3"].options["bucket"],
            serde_json::json!("{{org}}-example-state")
        );
    }

    #[test]
    fn env_override_wins_over_file_and_selects_org() {
        let mut env = HashMap::new();
        env.insert("CUBTERA_ORG".to_string(), "teracub".to_string());
        env.insert("CUBTERA_DB".to_string(), "mongodb://localhost".to_string());
        let src = StaticConfigSource {
            file_contents: Some(
                r#"
                [default]
                orgs = ["cubtera", "teracub"]
                "#
                .to_string(),
            ),
            env,
        };

        let config = ConfigProvider::load(&src).unwrap();
        assert_eq!(config.org, "teracub");
        assert_eq!(
            config.mongodb_connection_string,
            Some("mongodb://localhost".to_string())
        );
    }
}
