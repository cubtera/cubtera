mod manifest;
use manifest::Manifest;

use serde_json::json;
use serde_json::Value;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use yansi::Paint;

use crate::globals::GLOBAL_CFG;
use crate::prelude::data::Storage;
use crate::prelude::*;
use crate::utils::helper::*;

#[derive(Debug, Clone)]
pub struct Unit {
    pub name: String,
    pub manifest: Manifest,
    pub temp_folder: PathBuf,
    pub extensions: Vec<String>,
    pub dimensions: Vec<Dim>,
    opt_dims: Option<Vec<Dim>>,
    unit_folder: PathBuf,
    generic_unit_folder: Option<PathBuf>,
}

impl Unit {
    pub fn new(
        name: String,
        dimensions: &[String],
        extensions: &[String],
        storage: &Storage,
        context: Option<String>,
    ) -> Self {
        // Check if unit exists
        let mut unit_folder = Path::new(&GLOBAL_CFG.units_path).join(&name);
        let org_unit_folder = Path::new(&GLOBAL_CFG.units_path)
            .join(&GLOBAL_CFG.org)
            .join(&name);

        let (manifest, generic_unit_folder) = match Manifest::load(&org_unit_folder) {
            Ok(manifest) => {
                let generic_unit_folder = Manifest::load(&unit_folder)
                    .ok()
                    .map(|_| unit_folder.clone());

                unit_folder.clone_from(&org_unit_folder);
                (manifest, generic_unit_folder)
            }
            Err(e) => match Manifest::load(&unit_folder) {
                Ok(manifest) => {
                    debug!(target: "", "Unit {name} can't be load from org folder: {org_unit_folder:?} with error: {e}. Using generic unit from {unit_folder:?}");
                    (manifest, None)
                }
                Err(e) => exit_with_error(format!(
                    "Can't proceed with unit {}. {e}, ", name.red(),
                )),
            },
        };

        // Check if all required dimensions were provided
        manifest.dimensions.iter().for_each(|dim| {
            if !dimensions.iter().any(|x| x.starts_with(dim)) {
                exit_with_error(format!("Required dimension [{dim}] was not provided.",))
            }
        });

        // Sort dimensions by order in manifest.dimensions list
        let mut sorted_dimensions = dimensions.to_vec();
        sorted_dimensions.sort_by_key(|dim| {
            manifest
                .dimensions
                .iter()
                .position(|x| dim.starts_with(x))
                .unwrap_or(manifest.dimensions.len())
        });

        // only required dimensions (defined in manifest.dimensions list)
        let mut provided_required_dims = sorted_dimensions[0..manifest.dimensions.len()].to_vec();

        // other provided dimensions from cli if any
        let other_dims = sorted_dimensions[manifest.dimensions.len()..].to_vec();

        // ignore any other dimension, except optional add them to unit dimensions list
        if let Some(opt_dims) = manifest.opt_dims.clone() {
            let provided_optional_dims: Vec<String> = other_dims
                .iter()
                .filter(|&dim| {
                    opt_dims.contains(&dim.split_terminator(':').next().unwrap().to_string())
                })
                .map(|dim| dim.to_string())
                .collect();
            provided_required_dims.extend(provided_optional_dims);
        }

        // define all optional dimensions from manifest.optDims for empty values generation
        let opt_dims: Option<Vec<Dim>> = manifest.opt_dims.clone().map(|opt_dims| {
            opt_dims
                .iter()
                .map(|dim_type| DimBuilder::new_undefined(dim_type).build())
                .collect::<Vec<Dim>>()
        });
        let temp_folder = Path::new(&GLOBAL_CFG.temp_folder_path)
            .join(&GLOBAL_CFG.org)
            .join(&name)
            .join(provided_required_dims.join("/"))
            .join(extensions.join("/"));

        let extensions = extensions.to_vec();
        let dimensions = Unit::get_dims_from_cli(&provided_required_dims, storage, context);

        Unit {
            name,
            manifest,
            temp_folder,
            unit_folder,
            generic_unit_folder,
            dimensions,
            extensions,
            opt_dims,
        }
    }

    #[must_use]
    pub fn build(self) -> Self {
        // list all unit's dimensions with their parents
        let dims_set: HashSet<_> = self
            .dimensions
            .iter()
            .flat_map(|dim| dim.get_dim_tree())
            .collect();
        // check if minimum one of provided dims is allowed (included parent dim)
        if let Some(allow_list) = &self.manifest.allow_list {
            let allow_set: HashSet<_> = allow_list.clone().into_iter().collect();
            let allow_intersect: Vec<_> = allow_set.intersection(&dims_set).collect();
            if allow_intersect.is_empty() {
                warn!(target: "", "Any of provided dims {dims_set:?} was not ALLOWED for this unit. Check unit manifest 'allowList': {allow_list:?}. Execution terminated...");
                std::process::exit(0);
            }
        }
        // check if even one provided dims is denied (included parent dim)
        if let Some(deny_list) = &self.manifest.deny_list {
            let deny_set: HashSet<_> = deny_list.clone().into_iter().collect();
            let deny_set_intersect: Vec<_> = deny_set.intersection(&dims_set).collect();
            if !deny_set_intersect.is_empty() {
                warn!(target: "", "Some of provided dims {dims_set:?} was DENIED for this unit. Check unit manifest 'denyList': {deny_list:?}. Execution terminated...");
                std::process::exit(0);
            }
        }

        // check if all provided dims have required infra tags
        if let Some(affinity_tags) =
            self.dimensions.first().unwrap().get_dim_data()["meta"].get("affinity_tags")
        {
            let allowed_tags = affinity_tags
                .as_array()
                .unwrap()
                .iter()
                .map(|attribute: &Value| attribute.as_str().unwrap())
                .collect::<Vec<&str>>();
            if let Some(unit_tags) = &self.manifest.affinity_tags {
                if value_intersection(affinity_tags.clone(), json!(unit_tags)).is_none() {
                    warn!(target: "", "Unit {:?} doesn't have required affinity tags. Allowed tags: {allowed_tags:?}. Execution terminated...", self.name);
                    std::process::exit(0);
                }
                self.dimensions.iter().for_each(|dim| {
                    let dim_data = dim.get_dim_data();
                    let dim_tags = dim_data["meta"].get("affinity_tags").cloned().unwrap_or_default();
                    if value_intersection(affinity_tags.clone(), dim_tags.clone()).is_none(){
                        warn!(target: "", "Dimension {:?} doesn't have required affinity tags. Allowed tags: {allowed_tags:?}. Execution terminated...", dim_data["name"].as_str().unwrap_or_default());
                        std::process::exit(0);
                    }
                });
            } else {
                warn!(target: "", "Unit {:?} doesn't have required affinity tags. Allowed tags: {allowed_tags:?}. Execution terminated...", self.name.blue());
                std::process::exit(0);
            }
        }

        self
    }

    fn get_dims_from_cli(
        dim_names: &[String],
        storage: &Storage,
        context: Option<String>,
    ) -> Vec<Dim> {
        dim_names
            .iter()
            .map(|dim| DimBuilder::new_from_cli(dim, &GLOBAL_CFG.org, storage, context.clone()))
            .collect::<Vec<Dim>>()
    }

    pub fn get_unit_state_path(&self) -> String {
        let mut dims = self
            .dimensions
            .iter()
            .map(|dim| dim.key_path.to_str().unwrap())
            .collect::<Vec<&str>>();
        let mut exts = self
            .extensions
            .iter()
            .map(|ext| ext.as_str())
            .collect::<Vec<&str>>();
        dims.append(&mut exts);

        dims.join("/")
    }

    pub fn remove_temp_folder(&self) {
        let path = self.temp_folder.clone();
        if path.exists() {
            std::fs::remove_dir_all(&path).unwrap_or_exit("Can't remove temp folder".to_string());
            debug!(target: "", "Temp folder was removed: \n{:?}", path);
        }
    }

    pub fn copy_files(&self) {
        // define destination temp folder
        let dest_folder = self.temp_folder.clone();
        debug!(target: "unit mod", "Copying files to temp folder: \n{:?}", dest_folder);
        if !dest_folder.exists() {
            std::fs::create_dir_all(&dest_folder)
                .unwrap_or_exit(format!("Can't create temp folder: {:?}", &dest_folder));
        }

        // --------- Modules --------- //
        // add modules symlink to $temp folder
        // if modules_path is absolute => current_dir will be overwritten by join method
        let modules_folder_path = std::env::current_dir()
            .unwrap()
            .join(&GLOBAL_CFG.modules_path);
        if !dest_folder.join("modules").exists() {
            std::os::unix::fs::symlink(modules_folder_path, dest_folder.join("modules"))
                .unwrap_or_exit("Failed to create modules symlink".to_string());
        };

        // --------- Plugins --------- //
        // TODO: move to runner specific logic
        // copy plugin folder from config to $HOME/.terraform.d/plugins folder if exists
        // is plugins_path absolute => current_dir will be overwritten by join method
        let plugins_folder_path = std::env::current_dir()
            .unwrap()
            .join(&GLOBAL_CFG.plugins_path);
        if plugins_folder_path.exists() {
            let home_dir = std::env::var("HOME").unwrap();
            copy_folder(
                plugins_folder_path,
                &Path::new(&home_dir).join(".terraform.d/plugins"),
                false,
            );
        } else {
            warn!(target: "", "Plugin folder {plugins_folder_path:?} does not exist. Check cubtera config. Passed...");
        }

        // --------- Unit --------- //
        // self.manifest.overrides.then( ||
        //     self.generic_unit_folder.clone().map(|source_folder|
        //         copy_folder(source_folder, &dest_folder, true)
        //     )
        // );

        if self.manifest.overwrite {
            // copy generic unit files to temp folder if set overrides: true in unit_manifest
            if let Some(generic_unit_folder) = self.generic_unit_folder.clone() {
                copy_folder(generic_unit_folder, &dest_folder, true);
            };
        }

        copy_folder(self.unit_folder.clone(), &dest_folder, true);

        // Generate ALL Opt Dim json files with NULL values for each unit's optional dimension
        self.opt_dims.iter().for_each(|dim| {
            dim.iter().for_each(|opt_dim| {
                opt_dim
                    .save_json_dim_vars(dest_folder.clone())
                    .unwrap_or_exit(format!(
                        "Failed to save json dim vars for dim: {:?}",
                        &dest_folder
                    ));
            });
        });

        // Generate Dim Variables json files with json values for each unit's dimension
        self.dimensions.iter().for_each(|dim| {
            dim.save_json_dim_vars(dest_folder.clone())
                .unwrap_or_exit(format!(
                    "Failed to save json dim vars for dim {}",
                    &dim.dim_name
                ));

            dim.save_dim_includes(dest_folder.clone())
                .unwrap_or_exit(format!(
                    "Failed to save dim includes for dim {}",
                    &dim.dim_name
                ));

            dim.save_dim_folders(dest_folder.clone())
                .unwrap_or_exit(format!(
                    "Failed to save dim folders for dim {}",
                    &dim.dim_name
                ));
        });

        if !&self.extensions.is_empty() {
            let mut ext_tf_vars = String::new();
            ext_tf_vars.push_str("{\n");
            for extension in &self.extensions {
                let mut ext = extension.split_terminator(':');
                ext_tf_vars.push_str(&format!(
                    "  \"ext_{}_name\": \"{}\",\n",
                    ext.next().unwrap(),
                    ext.next().unwrap()
                ));
            }
            // remove last comma
            ext_tf_vars.pop();
            // remove last CR
            ext_tf_vars.pop();
            ext_tf_vars.push_str("\n}\n");
            std::fs::write(dest_folder.join("cubtera_ext.json"), ext_tf_vars)
                .unwrap_or_exit("Failed to write cubtera_ext.json file".to_string());
        }

        // --------- Files form Unit Manifest --------- //
        // copy all files defined in unit_manifest required and optional sections
        if let Some(spec) = &self.manifest.spec {
            if let Some(spec_files) = &spec.files {
                if let Some(required) = spec_files.required.clone() {
                    copy_files_from_manifest(required, &dest_folder, |src| {
                        exit_with_error(format!(
                            "Required file {} from unit manifest doesn't exist.",
                            src.red()
                        ))
                    });
                };
                if let Some(optional) = spec_files.optional.clone() {
                    copy_files_from_manifest(
                        optional,
                        &dest_folder,
                        |src| warn!(target:"", "Optional file {} from unit manifest doesn't exist. Passed...", src.blue()),
                    );
                };
            }
        }
    }

    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    pub fn get_unit_blob_sha(&self) -> String {
        let sha = get_blob_sha_by_path(&self.unit_folder);
        sha.unwrap_or_else(|e| {
            warn!("Failed to get unit blob sha: {e}");
            "undefined".to_string()
        })
    }

    pub fn get_unit_commit_sha(&self) -> String {
        let sha = get_commit_sha_by_path(&self.unit_folder);
        sha.unwrap_or_else(|e| {
            warn!("Failed to get unit commit sha: {e}");
            "undefined".to_string()
        })
    }

    pub fn get_dims_blob_sha(&self) -> HashMap<String, String> {
        self.dimensions
            .iter()
            .map(|dim| (format!("{}:{}", dim.dim_type, dim.dim_name), dim.data_sha.clone()))
            .collect::<HashMap<String, String>>()
    }

    pub fn get_env_vars(&self) -> Option<HashMap<String, String>> {
        self.manifest.spec.as_ref()
            .and_then(|spec| spec.env_vars.as_ref())
            .map(|env_vars| {
                env_vars.optional
                    .iter()
                    .flatten()
                    .chain(env_vars.required.iter().flatten())
                    .map(|(_, v)| std::env::var(v).ok().map(|val| (v.clone(), val)))
                    .flatten()
                    .collect::<HashMap<String, String>>()
            })
    }
}

fn copy_files_from_manifest(
    files: HashMap<String, String>,
    unit_folder: &Path,
    f: impl Fn(String),
) {
    for (src, dst) in files {
        let src_path = string_to_path(&src);
        if PathBuf::new().join(&src_path).exists() {
            let dest = unit_folder.join(dst);
            if !dest.exists() {
                std::fs::create_dir_all(dest.parent().unwrap())
                    .unwrap_or_exit(format!("Failed to create {dest:?} file"));
            }
            std::fs::copy(&src_path, dest)
                .unwrap_or_exit(format!("Failed to copy {} file", &src.red()));
        } else {
            f(src);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::dim::Dim;
    use std::fs;
    use std::io::Write;
    use tempfile::tempdir;

    // Helper to create a minimal Dim for testing
    // Note: Only sets public fields since dim_path and data are private
    fn create_test_dim(dim_type: &str, dim_name: &str) -> Dim {
        let mut dim = Dim::default();
        dim.dim_name = dim_name.to_string();
        dim.dim_type = dim_type.to_string();
        dim.key_path = PathBuf::from(format!("{}:{}", dim_type, dim_name));
        dim.data_sha = "test_sha".to_string();
        dim
    }

    // Helper to create a minimal Unit for testing
    fn create_test_unit(name: &str, dims: Vec<Dim>, extensions: Vec<String>) -> Unit {
        let manifest = Manifest {
            dimensions: dims.iter().map(|d| d.dim_type.clone()).collect(),
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: "tf".to_string(),
            spec: None,
            runner: None,
            state: None,
        };

        Unit {
            name: name.to_string(),
            manifest,
            temp_folder: PathBuf::from("/tmp/test"),
            extensions,
            dimensions: dims,
            opt_dims: None,
            unit_folder: PathBuf::from("/tmp/unit"),
            generic_unit_folder: None,
        }
    }

    #[test]
    fn test_get_name() {
        let unit = create_test_unit("my-unit", vec![], vec![]);
        assert_eq!(unit.get_name(), "my-unit");
    }

    #[test]
    fn test_get_unit_state_path_single_dim() {
        let dim = create_test_dim("env", "prod");
        let unit = create_test_unit("test-unit", vec![dim], vec![]);

        let state_path = unit.get_unit_state_path();
        assert_eq!(state_path, "env:prod");
    }

    #[test]
    fn test_get_unit_state_path_multiple_dims() {
        let dim1 = create_test_dim("env", "prod");
        let dim2 = create_test_dim("dc", "us-east-1");
        let unit = create_test_unit("test-unit", vec![dim1, dim2], vec![]);

        let state_path = unit.get_unit_state_path();
        assert_eq!(state_path, "env:prod/dc:us-east-1");
    }

    #[test]
    fn test_get_unit_state_path_with_extensions() {
        let dim = create_test_dim("env", "prod");
        let unit = create_test_unit(
            "test-unit",
            vec![dim],
            vec!["index:0".to_string(), "replica:1".to_string()],
        );

        let state_path = unit.get_unit_state_path();
        assert_eq!(state_path, "env:prod/index:0/replica:1");
    }

    #[test]
    fn test_get_unit_state_path_empty() {
        let unit = create_test_unit("test-unit", vec![], vec![]);

        let state_path = unit.get_unit_state_path();
        assert_eq!(state_path, "");
    }

    #[test]
    fn test_get_dims_blob_sha() {
        let mut dim1 = create_test_dim("env", "prod");
        dim1.data_sha = "sha1".to_string();

        let mut dim2 = create_test_dim("dc", "us-east-1");
        dim2.data_sha = "sha2".to_string();

        let unit = create_test_unit("test-unit", vec![dim1, dim2], vec![]);

        let shas = unit.get_dims_blob_sha();

        assert_eq!(shas.len(), 2);
        assert_eq!(shas.get("env:prod"), Some(&"sha1".to_string()));
        assert_eq!(shas.get("dc:us-east-1"), Some(&"sha2".to_string()));
    }

    #[test]
    fn test_get_env_vars_none() {
        let unit = create_test_unit("test-unit", vec![], vec![]);

        let env_vars = unit.get_env_vars();
        assert!(env_vars.is_none());
    }

    #[test]
    fn test_get_env_vars_with_spec() {
        use crate::core::unit::manifest::{EnvVars, Spec};

        // Set an env var for testing
        std::env::set_var("TEST_VAR_FOR_UNIT", "test_value");

        let mut required: HashMap<String, String> = HashMap::new();
        required.insert("test_key".to_string(), "TEST_VAR_FOR_UNIT".to_string());

        let env_vars = EnvVars {
            required: Some(required),
            optional: None,
        };

        let spec = Spec {
            tf_version: None,
            env_vars: Some(env_vars),
            files: None,
        };

        let manifest = Manifest {
            dimensions: vec![],
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: "tf".to_string(),
            spec: Some(spec),
            runner: None,
            state: None,
        };

        let unit = Unit {
            name: "test-unit".to_string(),
            manifest,
            temp_folder: PathBuf::from("/tmp/test"),
            extensions: vec![],
            dimensions: vec![],
            opt_dims: None,
            unit_folder: PathBuf::from("/tmp/unit"),
            generic_unit_folder: None,
        };

        let result = unit.get_env_vars();
        assert!(result.is_some());
        let vars = result.unwrap();
        assert_eq!(vars.get("TEST_VAR_FOR_UNIT"), Some(&"test_value".to_string()));

        // Clean up
        std::env::remove_var("TEST_VAR_FOR_UNIT");
    }

    #[test]
    fn test_unit_struct_fields() {
        let dim = create_test_dim("env", "prod");
        let unit = create_test_unit("my-unit", vec![dim.clone()], vec!["ext:val".to_string()]);

        assert_eq!(unit.name, "my-unit");
        assert_eq!(unit.extensions, vec!["ext:val"]);
        assert_eq!(unit.dimensions.len(), 1);
        assert_eq!(unit.dimensions[0].dim_name, "prod");
        assert_eq!(unit.manifest.unit_type, "tf");
        assert!(!unit.manifest.overwrite);
    }

    #[test]
    fn test_manifest_load_from_path() {
        let dir = tempdir().unwrap();
        let manifest_content = r#"
dimensions = ["env", "dc"]
type = "tf"
overwrite = true
"#;
        let manifest_path = dir.path().join("manifest.toml");
        let mut file = fs::File::create(&manifest_path).unwrap();
        write!(file, "{}", manifest_content).unwrap();

        let manifest = Manifest::load(dir.path()).unwrap();

        assert_eq!(manifest.dimensions, vec!["env", "dc"]);
        assert_eq!(manifest.unit_type, "tf");
        assert!(manifest.overwrite);
    }

    #[test]
    fn test_remove_temp_folder_nonexistent() {
        let unit = create_test_unit("test-unit", vec![], vec![]);
        // Should not panic when folder doesn't exist
        unit.remove_temp_folder();
    }

    #[test]
    fn test_remove_temp_folder_exists() {
        let dir = tempdir().unwrap();
        let temp_path = dir.path().join("unit_temp");
        fs::create_dir_all(&temp_path).unwrap();
        fs::write(temp_path.join("test.txt"), "test").unwrap();

        let manifest = Manifest {
            dimensions: vec![],
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: "tf".to_string(),
            spec: None,
            runner: None,
            state: None,
        };

        let unit = Unit {
            name: "test".to_string(),
            manifest,
            temp_folder: temp_path.clone(),
            extensions: vec![],
            dimensions: vec![],
            opt_dims: None,
            unit_folder: PathBuf::from("/tmp/unit"),
            generic_unit_folder: None,
        };

        assert!(temp_path.exists());
        unit.remove_temp_folder();
        assert!(!temp_path.exists());
    }

    #[test]
    fn test_copy_files_from_manifest_nonexistent() {
        let dir = tempdir().unwrap();
        let mut files: HashMap<String, String> = HashMap::new();
        files.insert("/nonexistent/file".to_string(), "dest.txt".to_string());

        // The callback is called for nonexistent files
        // Just verify the function doesn't panic and completes
        copy_files_from_manifest(files, dir.path(), |src| {
            // This should be called with the nonexistent source path
            assert_eq!(src, "/nonexistent/file");
        });

        // The destination file should NOT exist since source didn't exist
        let dest_file = dir.path().join("dest.txt");
        assert!(!dest_file.exists());
    }

    #[test]
    fn test_copy_files_from_manifest_existent() {
        let dir = tempdir().unwrap();
        let src_path = dir.path().join("source.txt");
        fs::write(&src_path, "test content").unwrap();

        let dest_dir = tempdir().unwrap();

        let mut files: HashMap<String, String> = HashMap::new();
        files.insert(src_path.to_str().unwrap().to_string(), "dest.txt".to_string());

        copy_files_from_manifest(files, dest_dir.path(), |_| {
            panic!("Should not be called for existing files");
        });

        let dest_file = dest_dir.path().join("dest.txt");
        assert!(dest_file.exists());
        assert_eq!(fs::read_to_string(dest_file).unwrap(), "test content");
    }

    #[test]
    fn test_unit_with_multiple_dimensions_state_path() {
        let mut dim1 = Dim::default();
        dim1.dim_name = "mgmt".to_string();
        dim1.dim_type = "dome".to_string();
        dim1.key_path = PathBuf::from("dome:mgmt");
        dim1.data_sha = "sha1".to_string();

        let mut dim2 = Dim::default();
        dim2.dim_name = "prod".to_string();
        dim2.dim_type = "env".to_string();
        dim2.key_path = PathBuf::from("dome:mgmt/env:prod");
        dim2.data_sha = "sha2".to_string();

        let mut dim3 = Dim::default();
        dim3.dim_name = "us-east-1".to_string();
        dim3.dim_type = "dc".to_string();
        dim3.key_path = PathBuf::from("dome:mgmt/env:prod/dc:us-east-1");
        dim3.data_sha = "sha3".to_string();

        let unit = create_test_unit("network", vec![dim1, dim2, dim3], vec![]);

        let state_path = unit.get_unit_state_path();
        assert_eq!(state_path, "dome:mgmt/dome:mgmt/env:prod/dome:mgmt/env:prod/dc:us-east-1");
    }

    #[test]
    fn test_unit_manifest_with_runner_config() {
        let mut runner: HashMap<String, String> = HashMap::new();
        runner.insert("version".to_string(), "1.5.0".to_string());
        runner.insert("state_backend".to_string(), "s3".to_string());

        let manifest = Manifest {
            dimensions: vec!["env".to_string()],
            overwrite: false,
            opt_dims: None,
            allow_list: None,
            deny_list: None,
            affinity_tags: None,
            unit_type: "tf".to_string(),
            spec: None,
            runner: Some(runner),
            state: None,
        };

        let unit = Unit {
            name: "test".to_string(),
            manifest,
            temp_folder: PathBuf::from("/tmp"),
            extensions: vec![],
            dimensions: vec![],
            opt_dims: None,
            unit_folder: PathBuf::from("/tmp/unit"),
            generic_unit_folder: None,
        };

        assert!(unit.manifest.runner.is_some());
        let runner = unit.manifest.runner.as_ref().unwrap();
        assert_eq!(runner.get("version"), Some(&"1.5.0".to_string()));
    }

    #[test]
    fn test_unit_extensions_parsing() {
        let unit = create_test_unit(
            "test",
            vec![],
            vec![
                "index:0".to_string(),
                "replica:primary".to_string(),
                "shard:1".to_string(),
            ],
        );

        assert_eq!(unit.extensions.len(), 3);
        assert!(unit.extensions.contains(&"index:0".to_string()));
        assert!(unit.extensions.contains(&"replica:primary".to_string()));
        assert!(unit.extensions.contains(&"shard:1".to_string()));
    }
}
