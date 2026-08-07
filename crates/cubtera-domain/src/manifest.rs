//! Unit manifest types
//!
//! A manifest defines how a unit should be executed. The TOML schema mirrors
//! v1's `manifest.toml` as-is - like the inventory naming convention, this is
//! a stable per-unit config format, not something v2 gets to redesign. The
//! one intentional break: `spec.tfVersion` (deprecated in v1, superseded by
//! `[runner] version`) is dropped.

use crate::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Runner type for the unit, derived from the manifest's `type` field.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum RunnerType {
    /// Terraform runner
    #[default]
    Terraform,
    /// OpenTofu runner
    OpenTofu,
    /// Bash script runner
    Bash,
    /// Helm chart runner (wave 2)
    Helm,
    /// Unknown runner type
    Unknown(String),
}

impl RunnerType {
    /// Parse runner type from string
    #[allow(clippy::should_implement_trait)] // infallible, not `FromStr`
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tf" | "terraform" => Self::Terraform,
            "tofu" | "opentofu" => Self::OpenTofu,
            "bash" | "sh" => Self::Bash,
            "helm" => Self::Helm,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::Terraform => "tf",
            Self::OpenTofu => "tofu",
            Self::Bash => "bash",
            Self::Helm => "helm",
            Self::Unknown(s) => s,
        }
    }
}

/// Unit manifest configuration, parsed as-is from `manifest.toml`.
///
/// Field names follow v1's camelCase-in-TOML convention (`optDims`,
/// `allowList`, `denyList`, `affinityTags`); snake_case aliases are accepted
/// too since some example units already use them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Manifest {
    /// Required dimension types, in order
    pub dimensions: Vec<String>,
    /// Whether to merge with the generic (org-less) unit of the same name
    #[serde(default)]
    pub overwrite: bool,
    /// Optional dimension types (accepted from CLI but not required)
    #[serde(alias = "opt_dims")]
    pub opt_dims: Option<Vec<String>>,
    /// If set, at least one provided dimension (or ancestor) must be in this list
    #[serde(alias = "allow_list")]
    pub allow_list: Option<Vec<String>>,
    /// If set, no provided dimension (or ancestor) may be in this list
    #[serde(alias = "deny_list")]
    pub deny_list: Option<Vec<String>>,
    /// If set, every provided dimension's own `meta.affinity_tags` must
    /// overlap with this list (see [`crate::AccessPolicy`])
    #[serde(alias = "affinity_tags")]
    pub affinity_tags: Option<Vec<String>>,
    /// Runner type: "tf"/"terraform", "tofu"/"opentofu", "bash"/"sh"
    #[serde(rename = "type")]
    pub unit_type: String,
    /// Legacy spec block (env vars / files to copy into the unit's temp folder)
    pub spec: Option<Spec>,
    /// Arbitrary runner config, e.g. `version`, `runner_command`,
    /// `inlet_command`, `outlet_command` (per-runner-type meaning, see
    /// `cubtera-runners`)
    pub runner: Option<HashMap<String, String>>,
    /// Arbitrary state backend config, e.g. `bucket`, `path` (per-backend
    /// meaning, rendered via handlebars in the state config template)
    pub state: Option<HashMap<String, String>>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            dimensions: Vec::new(),
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: RunnerType::default().as_str().to_string(),
            spec: None,
            runner: None,
            state: None,
        }
    }
}

impl Manifest {
    /// Create a new manifest with required dimensions and a runner type
    pub fn new(dimensions: Vec<String>, unit_type: impl Into<String>) -> Self {
        Self {
            dimensions,
            unit_type: unit_type.into(),
            ..Default::default()
        }
    }

    /// Parse a manifest from `manifest.toml` content. Pure function - the
    /// adapter is responsible for reading the file, this only parses it.
    pub fn from_toml(content: &str) -> DomainResult<Manifest> {
        toml::from_str(content).map_err(|e| DomainError::InvalidManifest {
            reason: e.to_string(),
        })
    }

    /// Runner type parsed from the manifest's `type` field
    pub fn runner_type(&self) -> RunnerType {
        RunnerType::from_str(&self.unit_type)
    }

    /// Check if a dimension name is allowed
    pub fn is_dimension_allowed(&self, name: &str) -> bool {
        if let Some(deny_list) = &self.deny_list {
            if deny_list.iter().any(|d| d == name) {
                return false;
            }
        }

        if let Some(allow_list) = &self.allow_list {
            return allow_list.iter().any(|a| a == name);
        }

        true
    }

    /// Check if a dimension type is required
    pub fn is_dimension_required(&self, dim_type: &str) -> bool {
        self.dimensions.iter().any(|d| d == dim_type)
    }

    /// Check if a dimension type is optional
    pub fn is_dimension_optional(&self, dim_type: &str) -> bool {
        self.opt_dims
            .as_ref()
            .is_some_and(|dims| dims.iter().any(|d| d == dim_type))
    }

    /// Runner version from the `[runner]` block, if set
    pub fn runner_version(&self) -> Option<&str> {
        self.runner.as_ref()?.get("version").map(|s| s.as_str())
    }

    /// State backend type from the `[runner]` block, if set
    pub fn state_backend(&self) -> Option<&str> {
        self.runner
            .as_ref()?
            .get("state_backend")
            .map(|s| s.as_str())
    }
}

/// Legacy spec block: extra env vars and files to make available to the unit
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Spec {
    /// Environment variables to forward into the runner process
    #[serde(alias = "envVars")]
    pub env_vars: Option<EnvVars>,
    /// Files to copy into the unit's temp folder before running
    pub files: Option<Files>,
}

/// Required/optional environment variables: manifest key -> host env var name
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct EnvVars {
    /// Missing entries only produce a warning
    pub optional: Option<HashMap<String, String>>,
    /// Missing entries are a hard error (materialization fails)
    pub required: Option<HashMap<String, String>>,
}

/// Required/optional files: source path (on the runner host) -> dest name
/// inside the unit's temp folder
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Files {
    /// Missing sources only produce a warning
    pub optional: Option<HashMap<String, String>>,
    /// Missing sources are a hard error (materialization fails)
    pub required: Option<HashMap<String, String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_type_from_str() {
        assert_eq!(RunnerType::from_str("tf"), RunnerType::Terraform);
        assert_eq!(RunnerType::from_str("terraform"), RunnerType::Terraform);
        assert_eq!(RunnerType::from_str("tofu"), RunnerType::OpenTofu);
        assert_eq!(RunnerType::from_str("bash"), RunnerType::Bash);
        assert_eq!(RunnerType::from_str("helm"), RunnerType::Helm);
        assert!(matches!(
            RunnerType::from_str("unknown"),
            RunnerType::Unknown(_)
        ));
    }

    #[test]
    fn test_manifest_dimension_allowed() {
        let mut manifest = Manifest::default();

        assert!(manifest.is_dimension_allowed("prod"));

        manifest.deny_list = Some(vec!["dev".to_string()]);
        assert!(manifest.is_dimension_allowed("prod"));
        assert!(!manifest.is_dimension_allowed("dev"));

        manifest.deny_list = None;
        manifest.allow_list = Some(vec!["prod".to_string(), "staging".to_string()]);
        assert!(manifest.is_dimension_allowed("prod"));
        assert!(!manifest.is_dimension_allowed("dev"));
    }

    #[test]
    fn test_manifest_required_dimensions() {
        let manifest = Manifest::new(vec!["env".to_string(), "dc".to_string()], "tf");

        assert!(manifest.is_dimension_required("env"));
        assert!(manifest.is_dimension_required("dc"));
        assert!(!manifest.is_dimension_required("region"));
    }

    #[test]
    fn test_manifest_is_dimension_optional() {
        let mut manifest = Manifest::new(vec!["env".to_string()], "tf");
        assert!(!manifest.is_dimension_optional("region"));

        manifest.opt_dims = Some(vec!["region".to_string(), "zone".to_string()]);
        assert!(manifest.is_dimension_optional("region"));
        assert!(!manifest.is_dimension_optional("dc"));
    }

    #[test]
    fn test_manifest_runner_type_from_toml_type_field() {
        let manifest = Manifest::new(vec!["env".to_string()], "tofu");
        assert_eq!(manifest.runner_type(), RunnerType::OpenTofu);
    }

    #[test]
    fn test_from_toml_basic() {
        let toml = r#"
dimensions = ["env", "dc"]
type = "tf"
"#;
        let manifest = Manifest::from_toml(toml).unwrap();
        assert_eq!(manifest.dimensions, vec!["env", "dc"]);
        assert_eq!(manifest.unit_type, "tf");
        assert!(!manifest.overwrite);
        assert!(manifest.opt_dims.is_none());
    }

    #[test]
    fn test_from_toml_camel_case_lists() {
        let toml = r#"
dimensions = ["env"]
type = "tf"
overwrite = true
optDims = ["region", "zone"]
allowList = ["prod", "staging"]
denyList = ["dev"]
affinityTags = ["core"]
"#;
        let manifest = Manifest::from_toml(toml).unwrap();
        assert!(manifest.overwrite);
        assert_eq!(
            manifest.opt_dims,
            Some(vec!["region".to_string(), "zone".to_string()])
        );
        assert_eq!(
            manifest.allow_list,
            Some(vec!["prod".to_string(), "staging".to_string()])
        );
        assert_eq!(manifest.deny_list, Some(vec!["dev".to_string()]));
        assert_eq!(manifest.affinity_tags, Some(vec!["core".to_string()]));
    }

    #[test]
    fn test_from_toml_snake_case_aliases() {
        let toml = r#"
dimensions = ["env"]
type = "tf"
opt_dims = ["region"]
allow_list = ["prod"]
deny_list = ["dev"]
affinity_tags = ["core"]
"#;
        let manifest = Manifest::from_toml(toml).unwrap();
        assert_eq!(manifest.opt_dims, Some(vec!["region".to_string()]));
        assert_eq!(manifest.allow_list, Some(vec!["prod".to_string()]));
        assert_eq!(manifest.deny_list, Some(vec!["dev".to_string()]));
        assert_eq!(manifest.affinity_tags, Some(vec!["core".to_string()]));
    }

    #[test]
    fn test_from_toml_runner_and_state_blocks() {
        let toml = r#"
dimensions = ["env"]
type = "tofu"

[runner]
version = "1.5.0"
state_backend = "s3"
runner_command = "/usr/bin/tofu"

[state]
bucket = "my-bucket"
region = "us-east-1"
"#;
        let manifest = Manifest::from_toml(toml).unwrap();
        assert_eq!(manifest.runner_version(), Some("1.5.0"));
        assert_eq!(manifest.state_backend(), Some("s3"));
        assert_eq!(
            manifest.runner.as_ref().unwrap().get("runner_command"),
            Some(&"/usr/bin/tofu".to_string())
        );
        assert_eq!(
            manifest.state.as_ref().unwrap().get("bucket"),
            Some(&"my-bucket".to_string())
        );
    }

    #[test]
    fn test_from_toml_spec_env_vars_and_files() {
        let toml = r#"
dimensions = ["env"]
type = "tf"

[spec.envVars.required]
AWS_ACCESS_KEY_ID = "AWS_ACCESS_KEY_ID"

[spec.envVars.optional]
AWS_SESSION_TOKEN = "AWS_SESSION_TOKEN"

[spec.files.required]
"~/.ssh/id_rsa" = "id_rsa"
"#;
        let manifest = Manifest::from_toml(toml).unwrap();
        let spec = manifest.spec.unwrap();
        let env_vars = spec.env_vars.unwrap();
        assert_eq!(
            env_vars.required.unwrap().get("AWS_ACCESS_KEY_ID"),
            Some(&"AWS_ACCESS_KEY_ID".to_string())
        );
        assert_eq!(
            env_vars.optional.unwrap().get("AWS_SESSION_TOKEN"),
            Some(&"AWS_SESSION_TOKEN".to_string())
        );
        let files = spec.files.unwrap();
        assert_eq!(
            files.required.unwrap().get("~/.ssh/id_rsa"),
            Some(&"id_rsa".to_string())
        );
    }

    #[test]
    fn test_from_toml_missing_required_fields_errors() {
        let toml = "overwrite = true";
        assert!(Manifest::from_toml(toml).is_err());
    }

    #[test]
    fn test_from_toml_invalid_toml_errors() {
        assert!(Manifest::from_toml("this is not valid toml [[[").is_err());
    }
}
