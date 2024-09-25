mod manifest;
use manifest::Manifest;

use serde_json::json;
use std::collections::HashSet;
use std::collections::HashMap;
use serde_json::Value;
use std::path::{Path, PathBuf};
use yansi::Paint;

use crate::globals::GLOBAL_CFG;
use crate::utils::helper::*;
use crate::prelude::*;
use crate::prelude::data::Storage;

#[derive(Debug, Clone)]
pub struct Unit {
    pub name: String,
    pub manifest: Manifest,
    pub temp_folder: PathBuf,
    pub extensions: Vec<String>,
    dimensions: Vec<Dim>,
    opt_dims: Option<Vec<Dim>>,
    unit_folder: PathBuf,
    generic_unit_folder: Option<PathBuf>,
}

impl Unit {
    #[must_use]
    /// # Panics
    pub fn new(
        name: String,
        dimensions: &[String],
        extensions: &[String],
        storage: &Storage,
        context: Option<String>
    ) -> Self {
        // Check if unit exists
        let mut unit_folder = Path::new(&GLOBAL_CFG.units_path).join(&name);
        let org_unit_folder = Path::new(&GLOBAL_CFG.units_path).join(&GLOBAL_CFG.org).join(&name);

        let (manifest, generic_unit_folder) = match Manifest::load(&org_unit_folder) {
            Ok(manifest) => {
                let generic_unit_folder = Manifest::load(&unit_folder)
                    .ok()
                    .map(|_| unit_folder.clone());

                unit_folder.clone_from(&org_unit_folder);
                (manifest, generic_unit_folder)
            },
            Err(e) => match Manifest::load(&unit_folder) {
                Ok(manifest) => {
                    debug!(target: "", "Unit {name} can't be load from org folder: {org_unit_folder:?} with error: {e}. Using generic unit from {unit_folder:?}");
                    (manifest, None)
                },
                Err(e) => exit_with_error(format!(
                    "Can't find unit {name}. Provide correct unit name. {e}",
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
        //dbg!(dimensions.clone());

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
        if let Some(affinity_tags) = self.dimensions.first().unwrap().get_dim_data()["meta"].get("affinity_tags") {
            let allowed_tags = affinity_tags.as_array().unwrap().iter()
                .map(|attribute: &Value| attribute.as_str().unwrap()).collect::<Vec<&str>>();
            if let Some(unit_tags) = &self.manifest.affinity_tags {
                if value_intersection(affinity_tags.clone(), json!(unit_tags)).is_none(){
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


    fn get_dims_from_cli(dim_names: &[String], storage: &Storage, context: Option<String>) -> Vec<Dim> {
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

    /// # Panics
    ///
    /// Will panic if there is a problem with files i/o
    pub fn remove_temp_folder(&self) {
        let path = self.temp_folder.clone();
        if path.exists() {
            std::fs::remove_dir_all(&path).unwrap_or_exit("Can't remove temp folder".to_string());
            debug!(target: "", "Temp folder was removed: \n{:?}", path);
        }
    }

    /// # Panics
    ///
    /// Will panic if there is a problem with files i/o
    pub fn copy_files(&self) {
        // define destination temp folder
        let dest_folder = self.temp_folder.clone();
        if !dest_folder.exists() {
            std::fs::create_dir_all(&dest_folder).unwrap_or_exit(
                format!("Can't create temp folder: {:?}", &dest_folder),
            );
        }
        // define possible source folder for org unit variant
        // let org_src_folder = Path::new(&CFG.units_path).join(&CFG.org).join(&self.name);

        // --------- Modules --------- //
        // add modules symlink to $temp folder
        // if modules_path is absolute => current_dir will be overwritten by join method
        let modules_folder_path = std::env::current_dir().unwrap().join(&GLOBAL_CFG.modules_path);
        if !dest_folder.join("modules").exists() {
            std::os::unix::fs::symlink(modules_folder_path, dest_folder.join("modules"))
                .unwrap_or_exit("Failed to create modules symlink".to_string());
        };

        // --------- Plugins --------- //
        // TODO: move to runner specific logic
        // copy plugin folder from config to $HOME/.terraform.d/plugins folder if exists
        // is plugins_path absolute => current_dir will be overwritten by join method
        let plugins_folder_path = std::env::current_dir().unwrap().join(&GLOBAL_CFG.plugins_path);
        if plugins_folder_path.exists() {
            let home_dir = std::env::var("HOME").unwrap();
            copy_folder(
                plugins_folder_path,
                &Path::new(&home_dir).join(".terraform.d/plugins"),
                false
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

        // --------- Generate ALL Opt Dim tf files with NULL json values --------- //
        self.opt_dims.iter().for_each(|dim| {
            dim.iter().for_each(|opt_dim| {
                opt_dim
                    .save_json_dim_vars(dest_folder.clone())
                    .unwrap_or_exit(format!(
                        "Failed to save json dim vars for dim: {:?}",
                        &dest_folder
                    ));
                // opt_dim
                //     .save_tf_dim_vars(dest_folder.clone())
                //     .unwrap_or_exit(format!(
                //         "Failed to save tf dim vars for dim: {:?}",
                //         &dest_folder
                //     ));
            });
        });

        // --------- Generate ALL unit Dim Variables tf files with json values --------- //
        self.dimensions.iter().for_each(|dim| {
            dim.save_json_dim_vars(dest_folder.clone())
                .unwrap_or_exit(format!(
                    "Failed to save json dim vars for dim {}",
                    &dim.dim_name
                ));
            // dim.save_tf_dim_vars(dest_folder.clone())
            //     .unwrap_or_exit(format!(
            //         "Failed to save tf dim vars for dim {}",
            //         &dim.dim_name
            //     ));
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

        // TODO: extension logic for all runners

        // --------- Generate Extension tf files --------- //
        // generate and copy ext_vars.tf file, if -e parameters was set for unit in format "ext_type:ext_name"
        // if !&self.extensions.is_empty() {
        //     let mut ext_tf_vars = String::new();
        //     for extension in &self.extensions {
        //         let mut ext = extension.split_terminator(':');
        //         ext_tf_vars.push_str(&format!(
        //             "variable \"ext_{}_name\" {{\n",
        //             ext.next().unwrap()
        //         ));
        //         ext_tf_vars.push_str(&format!("    default     = \"{}\"\n", ext.next().unwrap()));
        //         ext_tf_vars.push_str("    description = \"Generated by Cubtera\"\n");
        //         ext_tf_vars.push_str("}\n\n");
        //     }
        //     std::fs::write(dest_folder.join("ext_vars.tf"), ext_tf_vars)
        //         .unwrap_or_exit("Failed to write ext_vars.tf file".to_string());
        // }

        if !&self.extensions.is_empty() {
            let mut ext_tf_vars = String::new();
            ext_tf_vars.push_str("{\n");
            for extension in &self.extensions {
                let mut ext = extension.split_terminator(':');
                ext_tf_vars.push_str(&format!(
                    "  \"ext_{}_name\": \"{}\",\n",
                    ext.next().unwrap(), ext.next().unwrap()
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
                            "Required file {} from unit manifest doesn't exist.", src.red()
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
        // if let Some(spec_files) = &self.manifest.spec.files {
        //     if let Some(required) = spec_files.required.clone() {
        //         copy_files_from_manifest(required, &self.unit_folder, |src| {
        //             exit_with_error(format!(
        //                 "Required file {src} from unit_manifest doesn't exist."
        //             ))
        //         });
        //     };
        //     if let Some(optional) = spec_files.optional.clone() {
        //         copy_files_from_manifest(
        //             optional,
        //             &self.unit_folder,
        //             |src| warn!(target:"", "Optional file {src} defined in unit_manifest doesn't exist. Passed..."),
        //         );
        //     };
        // }
    }

    pub fn get_name(self) -> String {
        self.name
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
            std::fs::copy(&src_path, dest).unwrap_or_exit(format!("Failed to copy {} file", &src.red()));
        } else {
            f(src);
        }
    }
}
