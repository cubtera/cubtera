use std::collections::HashMap;
use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use log::{info, warn, debug};
use rand::Rng;
use yansi::Paint;
use serde_json::{json, Value};

mod tfswitch;

use tfswitch::tf_switch;
use super::{Runner, RunnerLoad};
use crate::prelude::*;

pub struct TfRunner {
    load: RunnerLoad,
    ctx: Value
}

impl Runner for TfRunner {
    fn new(load: RunnerLoad) -> Self {
        let ctx = Value::Object(serde_json::Map::new());
        TfRunner {
            load,
            ctx
        }
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

    fn copy_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        info!(target: "tf runner", "Copy files to: {}", &self.load.unit.temp_folder.to_string_lossy().blue());
        if self.load.command.first().unwrap_or(&"".into()).as_str() == "init"  {
            debug!(target: "tf runner", "Unit temp folder was created: \n{:?}", &self.load.unit.temp_folder);
            self.load.unit.remove_temp_folder();
            self.load.unit.copy_files();
            self.create_state_backend()?;
        } else {
            if !self.load.unit.temp_folder.exists() {
                exit_with_error(format!(
                    "Can't find unit temp folder {:?}. Run init command first.",
                    &self.load.unit.temp_folder
                ));
            }
            if GLOBAL_CFG.always_copy_files {
                self.load.unit.copy_files();
                self.create_state_backend()?;
            }
        }

        Ok(())
    }

    fn change_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "tf runner", "Change dimension data files into terraform format");

        // read all files started with dim_ and json extension
        let files = std::fs::read_dir(&self.load.unit.temp_folder)
            .unwrap_or_exit(format!("Can't read unit temp folder: {:?}", self.load.unit.temp_folder))
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|entry| entry.is_file())
            .filter(|entry| entry.extension().unwrap_or_default() == "json")
            // .filter(|entry| entry.file_stem().unwrap_or_default().to_str().unwrap_or_default().starts_with("dim_"))
            .filter(|entry| entry.file_stem().unwrap_or_default().to_str().unwrap_or_default().starts_with("cubtera_"))
            .filter(|entry| !entry.file_stem().unwrap_or_default().to_str().unwrap_or_default().contains(".auto.tfvars"))
            .collect::<Vec<std::path::PathBuf>>();

        // for each file read json as value and create list of root keys
        if !files.is_empty() {
            let dim_tf_variables: String = files.iter()
                .filter_map(|file| read_json_file(file))
                .filter_map(|json: serde_json::Value| json.as_object().map(|obj| obj.to_owned()))
                .flat_map(|obj| obj.into_iter().map(|(k, _)| k))
                .map(|key| format!(
r#"variable "{}" {{
    type        = any
    default     = null
    description = "Generated by Cubtera"
}}
"#,
                    key
                ))
                .collect();

            let mut file = std::fs::File::create(
                &self.load.unit.temp_folder.join("cubtera_vars.tf"))?;
            file.write_all(dim_tf_variables.as_bytes())?;
        }

        // rename all files to dim_<dim_name>.auto.tfvars.json
        files.iter().for_each(|file| {
            let new_file = file.with_file_name(
                format!("{}.auto.tfvars.json",
                    file.file_stem().unwrap_or_default().to_string_lossy() //.trim_start_matches("dim_")
                )
            );
            std::fs::rename(file, new_file).unwrap_or_exit(format!("Can't rename file: {:?}", file));
        });

        Ok(())
    }

    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let mut tf_args: Vec<String> = Vec::new();

        if let Some(command) = self.load.command.first() {
            match command.as_str() {
                "init" => {
                    // tf_args.extend(self.tf_backend_conf_args());
                    // debug!(target: "tf runner", "Unit temp folder was created: \n{:?}", &self.load.unit.temp_folder);
                }
                "plan" | "apply" | "destroy" | "refresh" => {
                    // check if unit temp folder is not empty
                    if !self.load.unit.temp_folder.exists() {
                        exit_with_error(format!(
                            "Can't find unit temp folder {:?}. Run init command first.",
                            &self.load.unit.temp_folder
                        ));
                    }
                    // extend vars with required env vars from unit manifest
                    tf_args.extend(self.tf_vars_args());
                }
                _ => {}
            }
        }

        // TODO: Remove legacy spec after units lib is fixed and remove params var usage
        let mut params = self.load.params.clone();
        if !self.load.unit.manifest.runner.is_some() {
            if let Some(spec) = &self.load.unit.manifest.spec {
                if let Some(version) = &spec.tf_version {
                    params.version = version.clone();
                    params.runner_command = None;
                    warn!(target: "tf runner", "{}: TF version is defined with {} in unit_manifest. \
                    Use {} instead.", "DEPRECATED".red(), "spec.tf_version".red(), "runner.version".blue());
                }
            }
        }

        let tf_path = match params.runner_command {
            Some(bin_path) => {
                info!(target: "tf runner", "Use custom binary path...");

                string_to_path(&bin_path)
                // Path::new(&bin_path).to_path_buf()
            },
            None => {
                info!(target: "tf runner", "Run terraform version {}", &params.version.yellow());
                tf_switch(&params.version).unwrap_or_exit(format!(
                    "Failed to switch to terraform version {}", params.version))
            }
        };

        if let Some(extra_params) = &self.load.params.extra_args {
            tf_args.extend(extra_params.split(" ").map(|s| s.to_string()));
        }

        info!(target: "tf runner", "Command: {} {}",
            tf_path.to_string_lossy().blue(),
            self.load.command.join(" ").blue(),
        );

        let filtered_env_vars: HashMap<String, String> = std::env::vars()
            .filter(
                |(k, _)| k.starts_with("TF_VAR_"), //|| k == "CUBTERA_TF_STATE_BUCKET_NAME"
            )
            .collect();

        // start terraform with all required arguments
        let mut tf_command = Command::new(&tf_path);

        let mut socket: Option<TcpListener> = None;
        // check if another instance is running with init and wait for it to finish
        if matches!(&self.load.command.as_slice(), [cmd, ..] if cmd == "init") {
            let delay = rand::thread_rng().gen_range(800..1200);

            // while socket.is_none() {
            //     socket = TcpListener::bind(("0.0.0.0", LOCK_PORT)).ok();
            //     info!(target: "tf runner", "Waiting for unlock while init in parallel");
            //     std::thread::sleep(std::time::Duration::from_millis(delay));
            // }

            loop {
                match TcpListener::bind(("0.0.0.0", self.load.params.get_lock_port())) {
                    Ok(listener) => {
                        socket = Some(listener);
                        break;
                    }
                    Err(_) => {
                        info!(target: "tf runner", "Waiting for unlock while init in parallel");
                        std::thread::sleep(std::time::Duration::from_millis(delay));
                    }
                }
            }
        };

        debug!(target: "tf runner", "Extra args: {}", &tf_args.join(" ").blue());

        let mut child = tf_command
            .current_dir(self.load.unit.temp_folder.to_str().unwrap())
            .args(&self.load.command)
            .args(tf_args)
            .envs(filtered_env_vars)
            .env("TF_VAR_org_name", &GLOBAL_CFG.org)
            .env("TF_VAR_unit_name", &self.load.unit.name)
            // TODO: remove after units lib is fixed (LEGACY)
            .env("TF_VAR_tf_state_s3bucket", &GLOBAL_CFG.tf_state_s3bucket)
            .env("TF_VAR_tf_state_s3region", &GLOBAL_CFG.tf_state_s3region)
            .env(
                "TF_VAR_tf_state_key_prefix",
                &GLOBAL_CFG.tf_state_key_prefix.clone().unwrap_or_default(),
            )
            //.env("TF_DATA_DIR", &self.config.temp_folder_path)
            //.env("TF_PLUGIN_CACHE_DIR", "~/.terraform.d/plugin-cache")
            //.env("TF_DATA_DIR", &self.config.temp_folder_path)
            //.env("TF_CLI_ARGS", "-compact-warnings")
            // TODO: implement different exit status configuration only for apply and plan commands:
            //.env("TF_CLI_ARGS","-detailed-exitcode")
            .env("TF_IN_AUTOMATION", "true")
            .env("TF_INPUT", "0")
            .spawn()
            .unwrap_or_exit(format!(
                "Failed to start {:?} with args {:?}",
                tf_path, &self.load.command
            ));

        let result = child
            .wait()
            .unwrap_or_exit("Failed to get terraform exitcode".to_string());

        let exit_code = result.code().unwrap_or(1);

        let tf_command = GLOBAL_CFG.dlog_db.clone()
            .and(matches!(self.load.command.as_slice(), [cmd, ..] if cmd == "apply")
                .then_some("apply")
                .or(matches!(self.load.command.as_slice(), [cmd, ..] if cmd == "destroy")
                    .then_some("destroy")));

        if let Some(tf_command) = tf_command {
            let dlog = Dlog::build(
                self.load.unit.clone(),
                tf_command.into(),
                exit_code,
            );
            let _ = dlog.put(&GLOBAL_CFG.org).check_with_warn("Can't put dlog to DB");
            info!(target: "tf runner", "Dlog data was saved");
        }

        if socket.is_some() {
            debug!(target: "tf runner", "Unlocking parallel run after finishing init command");
            drop(socket);
        }

        if !GLOBAL_CFG.clean_cache {
            debug!(target: "tf runner", "Ignore cache cleaning due to global config");
            //std::process::exit(exit_code);
            self.update_ctx("exit_code", json!(exit_code));

            return Ok(())
        }

        if exit_code == 0 {
            if let Some(first_command) = self.load.command.first() {
                if first_command == "apply"
                    || self.load.command.contains(&"--detailed-exitcode".to_owned())
                {
                    debug!(target: "tf runner", "Remove temp folder after successful {} command", "apply".blue());
                    self.load.unit.remove_temp_folder();
                }
            }
        }

        self.update_ctx("exit_code", json!(exit_code));
        Ok(())
        // std::process::exit(exit_code);
    }
}

impl TfRunner {

    fn create_state_backend(&self) -> Result<(), Box<dyn std::error::Error>> {
        // create tf backend config hcl file
        let tf_hcl = json!({
            "terraform": {
                "backend": &self.load.state_backend
            }
        });

        let path = self.load.unit.temp_folder.join("cubtera_backend.tf");
        convert_json_to_hcl_file(&tf_hcl, path)?;

        Ok(())
    }

    fn tf_vars_args(&self) -> Vec<String> {
        let mut tf_vars_args: Vec<String> = Vec::new();
        // extend vars with required env vars from unit manifest
        if let Some(spec) = &self.load.unit.manifest.spec {
            if let Some(env_vars) = &spec.env_vars {
                if let Some(required) = &env_vars.required {
                    for var in required {
                        tf_vars_args.extend([
                            "-var".to_string(),
                            format!(
                                "{}={}",
                                var.0,
                                std::env::var(var.1).unwrap_or_exit(format!("Required {}", var.1))
                            ),
                        ]);
                    }
                };
                if let Some(optional) = &env_vars.optional {
                    for var in optional {
                        if let Ok(env_var_value) = std::env::var(var.1) {
                            tf_vars_args
                                .extend(["-var".to_string(), format!("{}={env_var_value}", var.0)]);
                        }
                    }
                }
            }
        }

        tf_vars_args
    }
}

fn json_to_hcl(json: &Value, indent: usize) -> String {
    match json {
        Value::Object(map) => {
            let mut result = String::new();
            for (key, value) in map {
                let indentation = "  ".repeat(indent);
                match value {
                    Value::Object(inner_map) => {
                        if key == "backend" && inner_map.len() == 1 {
                            // Special handling for any backend type
                            let (backend_type, backend_config) = inner_map.iter().next().unwrap();
                            result.push_str(&format!("{}{}  \"{}\" {{\n", indentation, key, backend_type));
                            result.push_str(&json_to_hcl(backend_config, indent + 1));
                            result.push_str(&format!("{}}}\n", indentation));
                        } else {
                            result.push_str(&format!("{}{} {{\n", indentation, key));
                            result.push_str(&json_to_hcl(value, indent + 1));
                            result.push_str(&format!("{}}}\n", indentation));
                        }
                    }
                    Value::Array(arr) => {
                        result.push_str(&format!("{}{} = [\n", indentation, key));
                        for item in arr {
                            result.push_str(&format!("{}  {},\n", indentation, json_to_hcl(item, indent + 1).trim()));
                        }
                        result.push_str(&format!("{}]\n", indentation));
                    }
                    _ => {
                        result.push_str(&format!("{}{} = {}\n", indentation, key, json_to_hcl(value, indent)));
                    }
                }
            }
            result
        }
        Value::Array(arr) => {
            format!("[{}]", arr.iter().map(|v| json_to_hcl(v, indent)).collect::<Vec<_>>().join(", "))
        }
        Value::String(s) => format!("\"{}\"", s),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => "null".to_string(),
    }
}

fn convert_json_to_hcl_file(json: &Value, output_file: PathBuf) -> std::io::Result<()> {
    let hcl_content = json_to_hcl(json, 0);
    std::fs::write(output_file, hcl_content.as_bytes())?;
    Ok(())
}