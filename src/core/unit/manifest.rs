use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    pub dimensions: Vec<String>,
    #[serde(default)]
    pub overwrite: bool,
    #[serde(skip_serializing_if = "Option::is_none", alias = "opt_dims")]
    pub opt_dims: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "allow_list")]
    pub allow_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "deny_list")]
    pub deny_list: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "affinity_tags")]
    pub affinity_tags: Option<Vec<String>>,
    #[serde(rename(serialize = "type", deserialize = "type"))]
    pub unit_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub spec: Option<Spec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<HashMap<String, String>>,
}

impl Manifest {
    pub fn load(path: &Path) -> Result<Self> {
        let toml_path = path.join("manifest.toml");

        let toml = std::fs::read_to_string(&toml_path)
            .context(format!("Failed to read unit manifest at {:?}", toml_path))?;

        toml::from_str::<Manifest>(&toml)
            .context(format!("Failed to parse manifest at {:?}", toml_path))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
pub struct Spec {
    #[serde(skip_serializing_if = "Option::is_none", alias = "tfVersion")]
    pub tf_version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", alias = "envVars")]
    pub env_vars: Option<EnvVars>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<Files>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct EnvVars {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<HashMap<String, String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Files {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required: Option<HashMap<String, String>>,
}
