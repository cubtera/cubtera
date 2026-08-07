use std::path::PathBuf;
use std::process::Command;
use log::{debug, info};
use super::{Runner, RunnerLoad};
use serde_json::Value;
use crate::prelude::{read_json_file, ResultExtUnwrap};
use crate::tools::compat::LegacyCompat;

pub struct HelmRunner {
    load: RunnerLoad,
    ctx: Value,
}

impl Runner for HelmRunner {
    fn new(load: RunnerLoad) -> Self {
        let ctx = Value::Object(serde_json::Map::new());
        HelmRunner { load, ctx }
    }

    fn get_load(&self) -> &RunnerLoad {
        &self.load
    }

    fn get_ctx(&self) -> &Value {
        &self.ctx
    }

    fn get_ctx_mut(&mut self) -> &mut Value {
        &mut self.ctx
    }

    fn change_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "helm runner", "Generate values yaml from template and dimension's values");

        // read all files started with dim_ and json extension - using safe error handling
        let files = LegacyCompat::with_context(
            std::fs::read_dir(&self.load.unit.temp_folder),
            &format!(
                "Can't read unit temp folder: {:?}",
                self.load.unit.temp_folder
            ))?
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            .filter(|entry| {
                entry
                    .file_stem()
                    .unwrap_or_default()
                    .to_str()
                    .unwrap_or_default()
                    .starts_with("cubtera_")
            })
            .collect::<Vec<PathBuf>>();

        // for each file read json as value and create list of root keys
        #[allow(clippy::format_collect)]
        if !files.is_empty() {
            // let dim_tf_variables: Value = files
            //     .iter()
            //     .filter_map(read_json_file)
            //     // merge all json values
            //     .filter_map(|v| {
            //         if v.is_object() {
            //             Some(v)
            //         } else {
            //             None
            //         }
            //     }).collect();
            // dbg!(&dim_tf_variables);
            
            let mut dim_tf_variables = serde_json::Map::new();
            for file in files.iter().filter_map(read_json_file) {
                if let Some(obj) = file.as_object() {
                    for (key, value) in obj {
                        dim_tf_variables.insert(key.clone(), value.clone());
                    }
                }
            }
            let all_dims_variables = Value::Object(dim_tf_variables);

            let mut handlebars = handlebars::Handlebars::new();
            handlebars.set_strict_mode(true);
            
            //check template file exists
            let template_file = self.load.unit.temp_folder.join("values.yaml.tpl");
            if template_file.exists() {
                LegacyCompat::with_context(
                    handlebars.register_template_file("values.yaml", &template_file),
                    "Failed to register template file")?;
                
                // render template
                let rendered = LegacyCompat::with_context(
                    handlebars.render("values.yaml", &all_dims_variables),
                    "Failed to render template")?;

                // write rendered to file
                let mut output_file = self.load.unit.temp_folder.clone();
                output_file.push("values.yaml");
                LegacyCompat::with_context(
                    std::fs::write(&output_file, rendered.clone()),
                    &format!(
                        "Failed to write rendered template to file: {:?}",
                        output_file
                    ))?;
                dbg!(&rendered);
                
            } else {
                info!(target: "helm runner", "Values template file not found: {:?}", template_file);
            }
        }
        Ok(())
    }

    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!(target: "helm runner", "Run helm command");
        
        let mut child = LegacyCompat::with_context(
            Command::new("helm")
                .current_dir(self.load.unit.temp_folder.to_str().unwrap())
                .args(&self.load.command)
                .spawn(),
            &format!(
                "Failed to execute helm with args {:?}", &self.load.command
            ))?;
        
        let result = LegacyCompat::with_context(
            child.wait(),
            "Failed to get helm exitcode")?;
        
        let exit_code = result.code().unwrap_or(-1);
        
        // Use the common logger method from Runner trait
        self.logger(exit_code)?;
        
        if !result.success() {
            return Err(format!(
                "Helm command failed with exit code: {}",
                exit_code
            )
            .into());
        }
        
        Ok(())
    }
}
