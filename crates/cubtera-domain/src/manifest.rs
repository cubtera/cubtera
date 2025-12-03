//! Unit manifest types
//!
//! A manifest defines how a unit should be executed.

use std::collections::HashMap;

/// Runner type for the unit
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunnerType {
    /// Terraform runner
    Terraform,
    /// OpenTofu runner
    OpenTofu,
    /// Bash script runner
    Bash,
    /// Unknown runner type
    Unknown(String),
}

impl RunnerType {
    /// Parse runner type from string
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "tf" | "terraform" => Self::Terraform,
            "tofu" | "opentofu" => Self::OpenTofu,
            "bash" | "sh" => Self::Bash,
            other => Self::Unknown(other.to_string()),
        }
    }

    /// Get string representation
    pub fn as_str(&self) -> &str {
        match self {
            Self::Terraform => "tf",
            Self::OpenTofu => "tofu",
            Self::Bash => "bash",
            Self::Unknown(s) => s,
        }
    }
}

impl Default for RunnerType {
    fn default() -> Self {
        Self::Terraform
    }
}

/// Unit manifest configuration
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Required dimension types
    pub dimensions: Vec<String>,
    /// Optional dimension types
    pub optional_dimensions: Vec<String>,
    /// Runner type
    pub runner_type: RunnerType,
    /// Whether to merge with generic unit
    pub overwrite: bool,
    /// Allowed dimension values
    pub allow_list: Option<Vec<String>>,
    /// Denied dimension values
    pub deny_list: Option<Vec<String>>,
    /// Affinity tags for dimension matching
    pub affinity_tags: Option<Vec<String>>,
    /// Runner configuration
    pub runner_config: HashMap<String, String>,
    /// State backend configuration
    pub state_config: HashMap<String, String>,
    /// Spec configuration (legacy)
    pub spec: Option<ManifestSpec>,
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            dimensions: Vec::new(),
            optional_dimensions: Vec::new(),
            runner_type: RunnerType::default(),
            overwrite: false,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            runner_config: HashMap::new(),
            state_config: HashMap::new(),
            spec: None,
        }
    }
}

impl Manifest {
    /// Create a new manifest with required dimensions
    pub fn new(dimensions: Vec<String>, runner_type: RunnerType) -> Self {
        Self {
            dimensions,
            runner_type,
            ..Default::default()
        }
    }

    /// Check if a dimension name is allowed
    pub fn is_dimension_allowed(&self, name: &str) -> bool {
        // Check deny list first
        if let Some(deny_list) = &self.deny_list {
            if deny_list.iter().any(|d| d == name) {
                return false;
            }
        }

        // Check allow list
        if let Some(allow_list) = &self.allow_list {
            return allow_list.iter().any(|a| a == name);
        }

        // Default: allowed
        true
    }

    /// Check if a dimension type is required
    pub fn is_dimension_required(&self, dim_type: &str) -> bool {
        self.dimensions.iter().any(|d| d == dim_type)
    }

    /// Check if a dimension type is optional
    pub fn is_dimension_optional(&self, dim_type: &str) -> bool {
        self.optional_dimensions.iter().any(|d| d == dim_type)
    }

    /// Get runner version from config
    pub fn runner_version(&self) -> Option<&str> {
        self.runner_config.get("version").map(|s| s.as_str())
    }

    /// Get state backend type from config
    pub fn state_backend(&self) -> Option<&str> {
        self.runner_config.get("state_backend").map(|s| s.as_str())
    }
}

/// Legacy spec configuration
#[derive(Debug, Clone, Default)]
pub struct ManifestSpec {
    /// Terraform version (legacy, use runner_config instead)
    pub tf_version: Option<String>,
    /// Required environment variables
    pub required_env_vars: HashMap<String, String>,
    /// Optional environment variables
    pub optional_env_vars: HashMap<String, String>,
    /// Required files to copy
    pub required_files: HashMap<String, String>,
    /// Optional files to copy
    pub optional_files: HashMap<String, String>,
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
        assert!(matches!(RunnerType::from_str("unknown"), RunnerType::Unknown(_)));
    }

    #[test]
    fn test_manifest_dimension_allowed() {
        let mut manifest = Manifest::default();

        // No lists = everything allowed
        assert!(manifest.is_dimension_allowed("prod"));

        // With deny list
        manifest.deny_list = Some(vec!["dev".to_string()]);
        assert!(manifest.is_dimension_allowed("prod"));
        assert!(!manifest.is_dimension_allowed("dev"));

        // With allow list
        manifest.deny_list = None;
        manifest.allow_list = Some(vec!["prod".to_string(), "staging".to_string()]);
        assert!(manifest.is_dimension_allowed("prod"));
        assert!(!manifest.is_dimension_allowed("dev"));
    }

    #[test]
    fn test_manifest_required_dimensions() {
        let manifest = Manifest::new(
            vec!["env".to_string(), "dc".to_string()],
            RunnerType::Terraform,
        );

        assert!(manifest.is_dimension_required("env"));
        assert!(manifest.is_dimension_required("dc"));
        assert!(!manifest.is_dimension_required("region"));
    }
}

