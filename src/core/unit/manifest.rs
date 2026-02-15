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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    fn create_manifest_file(dir: &std::path::Path, content: &str) {
        let manifest_path = dir.join("manifest.toml");
        let mut file = fs::File::create(manifest_path).unwrap();
        write!(file, "{}", content).unwrap();
    }

    #[test]
    fn test_manifest_load_basic() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env", "dc"]
type = "tf"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert_eq!(manifest.dimensions, vec!["env", "dc"]);
        assert_eq!(manifest.unit_type, "tf");
        assert!(!manifest.overwrite);
        assert!(manifest.opt_dims.is_none());
        assert!(manifest.allow_list.is_none());
        assert!(manifest.deny_list.is_none());
    }

    #[test]
    fn test_manifest_load_with_overwrite() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["dome"]
type = "bash"
overwrite = true
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.overwrite);
        assert_eq!(manifest.unit_type, "bash");
    }

    #[test]
    fn test_manifest_load_with_allow_list() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
allowList = ["prod", "staging"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.allow_list.is_some());
        let allow_list = manifest.allow_list.unwrap();
        assert_eq!(allow_list, vec!["prod", "staging"]);
    }

    #[test]
    fn test_manifest_load_with_deny_list() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
denyList = ["dev", "test"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.deny_list.is_some());
        let deny_list = manifest.deny_list.unwrap();
        assert_eq!(deny_list, vec!["dev", "test"]);
    }

    #[test]
    fn test_manifest_load_with_opt_dims() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
optDims = ["region", "zone"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.opt_dims.is_some());
        let opt_dims = manifest.opt_dims.unwrap();
        assert_eq!(opt_dims, vec!["region", "zone"]);
    }

    #[test]
    fn test_manifest_load_with_affinity_tags() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
affinityTags = ["production", "critical"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.affinity_tags.is_some());
        let tags = manifest.affinity_tags.unwrap();
        assert_eq!(tags, vec!["production", "critical"]);
    }

    #[test]
    fn test_manifest_load_with_runner_config() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"

[runner]
version = "1.5.0"
state_backend = "s3"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.runner.is_some());
        let runner = manifest.runner.unwrap();
        assert_eq!(runner.get("version"), Some(&"1.5.0".to_string()));
        assert_eq!(runner.get("state_backend"), Some(&"s3".to_string()));
    }

    #[test]
    fn test_manifest_load_with_state_config() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"

[state]
bucket = "my-bucket"
region = "us-east-1"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.state.is_some());
        let state = manifest.state.unwrap();
        assert_eq!(state.get("bucket"), Some(&"my-bucket".to_string()));
        assert_eq!(state.get("region"), Some(&"us-east-1".to_string()));
    }

    #[test]
    fn test_manifest_load_with_spec_tf_version() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"

[spec]
tfVersion = "1.6.0"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.spec.is_some());
        let spec = manifest.spec.unwrap();
        assert_eq!(spec.tf_version, Some("1.6.0".to_string()));
    }

    #[test]
    fn test_manifest_load_with_spec_env_vars() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"

[spec.envVars.required]
AWS_ACCESS_KEY_ID = "AWS_ACCESS_KEY_ID"
AWS_SECRET_ACCESS_KEY = "AWS_SECRET_ACCESS_KEY"

[spec.envVars.optional]
AWS_SESSION_TOKEN = "AWS_SESSION_TOKEN"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.spec.is_some());
        let spec = manifest.spec.unwrap();
        assert!(spec.env_vars.is_some());

        let env_vars = spec.env_vars.unwrap();
        assert!(env_vars.required.is_some());
        assert!(env_vars.optional.is_some());

        let required = env_vars.required.unwrap();
        assert_eq!(required.get("AWS_ACCESS_KEY_ID"), Some(&"AWS_ACCESS_KEY_ID".to_string()));

        let optional = env_vars.optional.unwrap();
        assert_eq!(optional.get("AWS_SESSION_TOKEN"), Some(&"AWS_SESSION_TOKEN".to_string()));
    }

    #[test]
    fn test_manifest_load_with_spec_files() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"

[spec.files.required]
"~/.ssh/id_rsa" = "id_rsa"

[spec.files.optional]
"~/.ssh/id_rsa.pub" = "id_rsa.pub"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.spec.is_some());
        let spec = manifest.spec.unwrap();
        assert!(spec.files.is_some());

        let files = spec.files.unwrap();
        assert!(files.required.is_some());
        assert!(files.optional.is_some());

        let required = files.required.unwrap();
        assert_eq!(required.get("~/.ssh/id_rsa"), Some(&"id_rsa".to_string()));
    }

    #[test]
    fn test_manifest_load_complete() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["dome", "env", "dc"]
type = "tofu"
overwrite = true
allowList = ["mgmt", "stg"]
optDims = ["region"]
affinityTags = ["core"]

[runner]
state_backend = "local"
runner_command = "/usr/bin/tofu"
inlet_command = "echo inlet"
outlet_command = "echo outlet"

[state]
path = "~/.cubtera/state"

[spec]
tfVersion = "1.5.0"

[spec.envVars.required]
API_KEY = "API_KEY"

[spec.files.required]
"~/.config/file" = "config"
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert_eq!(manifest.dimensions, vec!["dome", "env", "dc"]);
        assert_eq!(manifest.unit_type, "tofu");
        assert!(manifest.overwrite);
        assert_eq!(manifest.allow_list, Some(vec!["mgmt".to_string(), "stg".to_string()]));
        assert_eq!(manifest.opt_dims, Some(vec!["region".to_string()]));
        assert_eq!(manifest.affinity_tags, Some(vec!["core".to_string()]));

        let runner = manifest.runner.unwrap();
        assert_eq!(runner.get("state_backend"), Some(&"local".to_string()));
        assert_eq!(runner.get("runner_command"), Some(&"/usr/bin/tofu".to_string()));
        assert_eq!(runner.get("inlet_command"), Some(&"echo inlet".to_string()));
        assert_eq!(runner.get("outlet_command"), Some(&"echo outlet".to_string()));

        let state = manifest.state.unwrap();
        assert_eq!(state.get("path"), Some(&"~/.cubtera/state".to_string()));
    }

    #[test]
    fn test_manifest_load_missing_file() {
        let dir = tempdir().unwrap();
        // Don't create the manifest file

        let result = Manifest::load(dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to read unit manifest"));
    }

    #[test]
    fn test_manifest_load_invalid_toml() {
        let dir = tempdir().unwrap();
        let content = "this is not valid toml [[[";
        create_manifest_file(dir.path(), content);

        let result = Manifest::load(dir.path());

        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("Failed to parse manifest"));
    }

    #[test]
    fn test_manifest_load_missing_required_fields() {
        let dir = tempdir().unwrap();
        // Missing 'dimensions' and 'type'
        let content = r#"
overwrite = true
"#;
        create_manifest_file(dir.path(), content);

        let result = Manifest::load(dir.path());

        assert!(result.is_err());
    }

    #[test]
    fn test_manifest_serialization_roundtrip() {
        let manifest = Manifest {
            dimensions: vec!["env".to_string(), "dc".to_string()],
            overwrite: true,
            opt_dims: Some(vec!["region".to_string()]),
            allow_list: Some(vec!["prod".to_string()]),
            deny_list: None,
            affinity_tags: None,
            unit_type: "tf".to_string(),
            spec: None,
            runner: None,
            state: None,
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let deserialized: Manifest = serde_json::from_str(&json).unwrap();

        assert_eq!(manifest.dimensions, deserialized.dimensions);
        assert_eq!(manifest.unit_type, deserialized.unit_type);
        assert_eq!(manifest.overwrite, deserialized.overwrite);
        assert_eq!(manifest.opt_dims, deserialized.opt_dims);
        assert_eq!(manifest.allow_list, deserialized.allow_list);
    }

    #[test]
    fn test_env_vars_struct() {
        let mut required: HashMap<String, String> = HashMap::new();
        required.insert("KEY1".to_string(), "VALUE1".to_string());

        let mut optional: HashMap<String, String> = HashMap::new();
        optional.insert("KEY2".to_string(), "VALUE2".to_string());

        let env_vars = EnvVars {
            required: Some(required.clone()),
            optional: Some(optional.clone()),
        };

        let json = serde_json::to_string(&env_vars).unwrap();
        let deserialized: EnvVars = serde_json::from_str(&json).unwrap();

        assert_eq!(env_vars, deserialized);
    }

    #[test]
    fn test_files_struct() {
        let mut required: HashMap<String, String> = HashMap::new();
        required.insert("src".to_string(), "dst".to_string());

        let files = Files {
            required: Some(required.clone()),
            optional: None,
        };

        let json = serde_json::to_string(&files).unwrap();
        let deserialized: Files = serde_json::from_str(&json).unwrap();

        assert_eq!(files, deserialized);
    }

    #[test]
    fn test_spec_struct() {
        let spec = Spec {
            tf_version: Some("1.5.0".to_string()),
            env_vars: None,
            files: None,
        };

        let json = serde_json::to_string(&spec).unwrap();
        let deserialized: Spec = serde_json::from_str(&json).unwrap();

        assert_eq!(spec.tf_version, deserialized.tf_version);
    }

    #[test]
    fn test_manifest_unit_types() {
        let test_cases = vec!["tf", "bash", "tofu"];

        for unit_type in test_cases {
            let dir = tempdir().unwrap();
            let content = format!(
                r#"
dimensions = ["env"]
type = "{}"
"#,
                unit_type
            );
            create_manifest_file(dir.path(), &content);

            let manifest = Manifest::load(dir.path()).unwrap();
            assert_eq!(manifest.unit_type, unit_type);
        }
    }

    #[test]
    fn test_manifest_alias_opt_dims() {
        // Test that both 'optDims' (camelCase) and 'opt_dims' (snake_case) work
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
opt_dims = ["region", "zone"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.opt_dims.is_some());
        assert_eq!(manifest.opt_dims.unwrap(), vec!["region", "zone"]);
    }

    #[test]
    fn test_manifest_alias_allow_list() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
allow_list = ["prod"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.allow_list.is_some());
        assert_eq!(manifest.allow_list.unwrap(), vec!["prod"]);
    }

    #[test]
    fn test_manifest_alias_deny_list() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
deny_list = ["dev"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.deny_list.is_some());
        assert_eq!(manifest.deny_list.unwrap(), vec!["dev"]);
    }

    #[test]
    fn test_manifest_alias_affinity_tags() {
        let dir = tempdir().unwrap();
        let content = r#"
dimensions = ["env"]
type = "tf"
affinity_tags = ["tag1"]
"#;
        create_manifest_file(dir.path(), content);

        let manifest = Manifest::load(dir.path()).unwrap();

        assert!(manifest.affinity_tags.is_some());
        assert_eq!(manifest.affinity_tags.unwrap(), vec!["tag1"]);
    }
}
