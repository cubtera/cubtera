mod bash;
pub mod params;
#[allow(clippy::option_map_unit_fn)]
mod tf;
mod tofu;
mod helm;


use crate::prelude::*;
use crate::tools::compat::{LegacyCompat, OptionCompat};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

// add new runner here
pub fn runner_create(runner_type: RunnerType, load: RunnerLoad) -> Box<dyn Runner> {
    match runner_type {
        RunnerType::TF => Box::new(tf::TfRunner::new(load)),
        RunnerType::BASH => Box::new(bash::BashRunner::new(load)),
        RunnerType::TOFU => Box::new(tofu::TofuRunner::new(load)),
        RunnerType::HELM => Box::new(helm::HelmRunner::new(load)),
        _ => exit_with_error(format!(
            "Unknown runner type: {runner_type:?}. Check documentation about supported runners"
        )),
    }
}

#[derive(Debug)]
pub struct RunnerLoad {
    unit: Unit,
    command: Vec<String>,         // command from cli
    params: params::RunnerParams, // HashMap<String, String>, // runner params from unit manifest and global config
    state_backend: Value,
}

#[derive(Debug, Clone)]
pub enum RunnerType {
    TF,
    BASH,
    TOFU,
    HELM,
    UNKNOWN,
}

impl RunnerType {
    pub fn str_to_runner_type(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "TF" => RunnerType::TF,
            "BASH" => RunnerType::BASH,
            "TOFU" => RunnerType::TOFU,
            "HELM" => RunnerType::HELM,
            _ => RunnerType::UNKNOWN,
        }
    }
}

pub trait Runner {
    fn new(load: RunnerLoad) -> Self
    where
        Self: Sized;
    fn get_load(&self) -> &RunnerLoad;
    fn get_ctx(&self) -> &Value;
    fn get_ctx_mut(&mut self) -> &mut Value;

    fn copy_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default copy_files method.");

        self.get_load().unit.remove_temp_folder();
        self.get_load().unit.copy_files();

        self.update_ctx("copy_files", json!("executed"));
        let working_dir = self
            .get_load()
            .unit
            .temp_folder
            .clone()
            .to_string_lossy()
            .to_string();
        self.update_ctx("working_dir", json!(working_dir));

        Ok(())
    }

    fn change_files(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default change_files method. Do nothing.");
        self.update_ctx("change_files", json!("passed"));
        Ok(())
    }

    fn inlet(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default inlet method.");
        self.executor("inlet")?;

        Ok(())
    }

    fn runner(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default runner method.");
        self.executor("runner")?;

        Ok(())
    }

    fn outlet(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default outlet method.");
        self.executor("outlet")?;

        Ok(())
    }

    /// Logs runner execution data to database or disk
    /// 
    /// # Behavior
    /// - If `GLOBAL_CFG.dlog_db.is_some()`: Saves to configured database
    /// - If `GLOBAL_CFG.dlog_db.is_none()`: Saves to disk at `~/.cubtera/{org}/{unit}/{dims}/dlog.json`
    /// 
    /// # Arguments
    /// * `exit_code` - The exit code from the command execution
    /// 
    /// # Returns
    /// * `Ok(())` - If logging was successful
    /// * `Err(Box<dyn std::error::Error>)` - If logging failed
    fn logger(&mut self, exit_code: i32) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default logger method.");
        
        // Log to database if configured and dlog_db is available
        if GLOBAL_CFG.dlog_db.is_some() {
            use crate::core::dlog::Dlog;
            
            // Get command type from the first command argument
            let command_type = self.get_load().command
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown");
                
            let dlog = Dlog::build(self.get_load().unit.clone(), command_type.into(), exit_code);
            let _ = dlog
                .put(&GLOBAL_CFG.org)
                .check_with_warn("Can't put dlog to DB");
            info!(target: "runner", "Dlog data was saved for {} command", command_type);
        } else {
            // Save to disk if no dlog_db configuration
            use crate::core::dlog::Dlog;
            
            // Get command type from the first command argument
            let command_type = self.get_load().command
                .first()
                .map(|s| s.as_str())
                .unwrap_or("unknown");
            
            // Build dlog data same as for DB
            let dlog = Dlog::build(self.get_load().unit.clone(), command_type.into(), exit_code);
            
            // Create disk path: ~/.cubtera/ + same path structure as temp_folder
            let home_dir = std::env::var("HOME")
                .unwrap_or_else(|_| "/tmp".to_string());
            
            // Get relative path from temp_folder (remove temp_folder_path prefix)
            let temp_folder = &self.get_load().unit.temp_folder;
            let temp_folder_path = Path::new(&GLOBAL_CFG.temp_folder_path);
            
            let relative_path = if let Ok(rel_path) = temp_folder.strip_prefix(temp_folder_path) {
                rel_path.to_path_buf()
            } else {
                // Fallback: construct path manually if strip_prefix fails
                Path::new(&GLOBAL_CFG.org)
                    .join(&self.get_load().unit.name)
                    .join(self.get_load().unit.get_unit_state_path())
            };
            
            // Create dlog file path: ~/.cubtera/{relative_path}/dlog.json
            let dlog_dir = Path::new(&home_dir)
                .join(".cubtera")
                .join(relative_path);
            
            let dlog_file_path = dlog_dir.join("dlog.json");
            
            // Create directory if it doesn't exist
            std::fs::create_dir_all(&dlog_dir)?;
            
            // Serialize dlog to JSON and save to file
            let dlog_json = serde_json::to_string_pretty(&dlog)?;
            std::fs::write(&dlog_file_path, dlog_json)?;
            
            info!(target: "runner", "Dlog data was saved to disk: {:?} for {} command", dlog_file_path, command_type);
        }
        
        self.update_ctx("logger", json!("executed"));
        self.update_ctx("exit_code", json!(exit_code));
        debug!(target: "runner", "Final context: {}", self.get_ctx().to_string());

        Ok(())
    }

    fn run(&mut self) -> Result<Value, Box<dyn std::error::Error>> {
        self.copy_files()?;
        self.change_files()?;
        self.inlet()?;
        self.runner()?;
        self.outlet()?;
        // Note: logger will be called by individual runners with their exit code
        
        Ok(self.get_ctx().clone())
    }

    fn update_ctx(&mut self, key: &str, value: Value) {
        let ctx = self.get_ctx_mut();
        ctx[key] = value;
    }

    fn executor(&mut self, step: &str) -> Result<(), Box<dyn std::error::Error>> {
        use yansi::Paint; // don't move this to the top - conflicts with a crate

        let dir = self
            .get_load()
            .unit
            .temp_folder
            .clone()
            .to_string_lossy()
            .to_string();

        let params = self.get_load().params.clone();
        let args = self.get_load().command.clone();

        let command = match step {
            "inlet" => params.inlet_command,
            "outlet" => params.outlet_command,
            "runner" => params
                .runner_command
                .and_then(|cmd| format!("{} {}", cmd, args.join(" ")).into())
                .and_then(|cmd| {
                    format!("{} {}", cmd, params.extra_args.unwrap_or_default()).into()
                }),
            _ => None,
        };

        if let Some::<String>(command) = command {
            self.update_ctx(step, json!(format!("{}", &command)));
            info!(target: "runner", "{} command: {}", capitalize_first(step), &command.blue());

            let mut env_vars = std::env::vars().collect::<HashMap<String, String>>();
            env_vars.insert("CUBTERA_RUNNER_CMD".into(), self.get_load().command.join(" "));

            let exit_code = execute_command(&command, &dir, env_vars)?;
            let exit_code_value = exit_code.code().unwrap_or(1);

            self.update_ctx(&format!("{}_exit_code", step), json!(exit_code_value));
            
            // Call logger for main runner step
            if step == "runner" {
                self.logger(exit_code_value)?;
            }
            
            if exit_code.success() {
                debug!(target: "runner", "{} command executed successfully", capitalize_first(step));
            } else {
                exit_with_error(format!("{} command failed with {}", capitalize_first(step), exit_code));
            }
        }

        Ok(())
    }
}

pub struct RunnerBuilder {
    unit: Unit,
    command: Vec<String>,
}

impl RunnerBuilder {
    pub fn new(unit: Unit, command: Vec<String>) -> Self {
        RunnerBuilder { unit, command }
    }

    pub fn build(&self) -> Box<dyn Runner> {
        let mut params = HashMap::new();
        let mut state_type = "";
        // let mut state_backend = Value::Null;

        if let Some(runner) = &GLOBAL_CFG.runner {
            // let config_runner = runner.get(&self.unit.manifest.unit_type);
            if let Some(config_runner_params) = runner.get(&self.unit.manifest.unit_type) {
                // read runner params from global config
                params.extend(config_runner_params.clone());

                // check if state type is defined in global config and overwrite default
                if let Some(state) = config_runner_params.get("state_backend") {
                    state_type = state;
                }
            }
        }

        if let Some(manifest_runner_params) = &self.unit.manifest.runner {
            // read runner params from unit manifest
            params.extend(manifest_runner_params.clone());

            // check if state type is defined in unit manifest and overwrite global config
            if let Some(state) = manifest_runner_params.get("state_backend") {
                state_type = state;
            }
        }

        if state_type.is_empty() {
            debug!(target: "runner", "State type is not defined in global config or unit manifest. Using default state type: 'local'");
            state_type = "local";
        }


        let state_backend = GLOBAL_CFG
            .clone()
            .state
            .and_then(|s| s.get(state_type).cloned())
            .or(self.unit.manifest.state.clone())
            .map_or_else(
                ||
                    // json!({
                    //     "local": {
                    //         "path": string_to_path("~/.cubtera/state/{{ org }}/{{ dim_tree }}/{{ unit_name }}.tfstate"),
                    //     }
                    // }),

                    // TODO: remove after migration to new state backend config
                    exit_with_error("State backend config is not defined in global config or unit manifest. \
                        Check documentation about supported state backends".into()),

                |state| {
                    if state_type == "local" {
                        json!({
                            "local": {
                                "path": string_to_path(state.get("path")
                                    .unwrap_or_else(|| exit_with_error("Local backend path is not defined right in global config or unit manifest".into()))),
                            }
                        })
                    } else { json!({ state_type: state}) }
            });

            // .map(|state| json!({ state_type: state }))
            // .unwrap_or(json!({
            //     "local": {
            //         "path": string_to_path("~/.cubtera/state/{{ org }}/{{ dim_tree }}/{{ unit_name }}.tfstate"),
            //     }
            // }));

        // // check if state type is defined in global config
        // if let Some(s) = GLOBAL_CFG.clone().state
        //     .and_then(|s| s.get(state_type).cloned()){
        //     state_backend = json!(s);
        // }
        //
        // // check if state type is defined in unit manifest
        // if let Some(state) = &self.unit.manifest.state {
        //     state_backend = json!(state.clone());
        // }
        // // self.unit.manifest.state.clone()
        // //     .map(|s| state_backend = json!(s));

        // let state_backend = match backend {
        //     None => {
        //         debug!(target: "runner", "State backend config is not defined in global config or unit manifest. Using default.");
        //         json!({
        //             "local": {
        //                 "path": string_to_path("~/.cubtera/state/{{ org }}/{{ dim_tree }}/{{ unit_name }}.tfstate"),
        //             }
        //         })
        //     }
        //     Some(backend) => json!({ state_type: backend }),
        // };

        // TODO: Move this to a separate function
        // apply handlebars template to state_backend definition
        let mut handlebars = handlebars::Handlebars::new();
        handlebars.set_strict_mode(true);

        // Add values for state_backend template rendering
        let data = json!({
            "org": &GLOBAL_CFG.org,
            "unit_name": &self.unit.name,
            "dim_tree": self.unit.get_unit_state_path(),
        });

        let state_backend = apply_template_to_value(&state_backend, &handlebars, &data);
        let params = params::RunnerParams::init(params);

        let load = RunnerLoad {
            unit: self.unit.clone(),
            command: self.command.clone(),
            params,
            state_backend,
        };

        let runner_type = RunnerType::str_to_runner_type(&self.unit.manifest.unit_type);
        runner_create(runner_type, load)
    }
}

pub fn apply_template_to_value(
    value: &Value,
    handlebars: &handlebars::Handlebars,
    data: &Value,
) -> Value {
    match value {
        Value::Object(map) => {
            let mut new_map = serde_json::Map::new();
            for (k, v) in map {
                new_map.insert(k.clone(), apply_template_to_value(v, handlebars, data));
            }
            Value::Object(new_map)
        }
        Value::Array(arr) => {
            let new_arr: Vec<Value> = arr
                .iter()
                .map(|v| apply_template_to_value(v, handlebars, data))
                .collect();
            Value::Array(new_arr)
        }
        Value::String(s) => {
            let templated = handlebars
                .render_template(s, data)
                .unwrap_or_else(|_| s.clone());
            Value::String(templated)
        }
        _ => value.clone(),
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod logger_tests;
