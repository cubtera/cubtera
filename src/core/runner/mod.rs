mod tf;
mod bash;
mod tofu;

use std::collections::HashMap;
use crate::prelude::*;


#[derive(Debug)]
pub struct RunnerContext {
    unit: Unit,
    command: Vec<String>, // command from cli
    params: HashMap<String, String> // runner params from unit manifest and global config
}

#[derive(Debug, Clone)]
pub enum RunnerType {
    TF,
    BASH,
    TOFU,
    UNKNOWN
}

impl RunnerType {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "TF" => RunnerType::TF,
            "BASH" => RunnerType::BASH,
            "TOFU" => RunnerType::TOFU,
            _ => RunnerType::UNKNOWN
        }
    }
}

pub trait Runner {

    fn new(ctx: RunnerContext) -> Self where Self: Sized;

    fn init(&self) -> Result<(), Box<dyn std::error::Error>>{
        debug!(target: "runner", "Default init method implementation.");

        self.copy_files()?;
        self.change_files()?;

        Ok(())
    }

    fn copy_files(&self) -> Result<(), Box<dyn std::error::Error>>{
        debug!(target: "runner", "Default copy_files method implementation. Do nothing.");

        Ok(())
    }

    fn change_files(&self) -> Result<(), Box<dyn std::error::Error>>{
        debug!(target: "runner", "Default change_files method implementation. Do nothing.");

        Ok(())
    }

    fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "runner", "Default run method implementation. Do nothing.");

        Ok(())
    }
}

pub struct RunnerBuilder {
    unit: Unit,
    command: Vec<String>
}

impl RunnerBuilder {
    pub fn new(unit: Unit, command: Vec<String>) -> Self {
        RunnerBuilder {
            unit,
            command
        }
    }

    pub fn build(&self) -> Box<dyn Runner> {
        let mut params = HashMap::new();

        if let Some(runner) = &GLOBAL_CFG.runner {
            let config_runner = runner.get(&self.unit.manifest.unit_type);
            if let Some(config_runner_params) = config_runner {
                params.extend(config_runner_params.clone());
            }
        }

        if let Some(manifest_runner_params) = &self.unit.manifest.runner {
            params.extend(manifest_runner_params.clone());
        }

        let ctx = RunnerContext {
            unit: self.unit.clone(),
            command: self.command.clone(),
            params
        };

        let runner_type = RunnerType::from_str(&self.unit.manifest.unit_type);
        runner_create(runner_type, ctx)
    }
}

// add new runner here
fn runner_create(runner_type: RunnerType, ctx: RunnerContext) -> Box<dyn Runner> {
    match runner_type {
        RunnerType::TF => Box::new(tf::TfRunner::new(ctx)),
        RunnerType::BASH => Box::new(bash::BashRunner::new(ctx)),
        RunnerType::TOFU => Box::new(tofu::TofuRunner::new(ctx)),
        _ => exit_with_error(format!("Unknown runner type: {runner_type:?}. Check documentation about supported runners"))
    }
}

trait RunnerParams {
    fn init(params: HashMap<String, String>) -> Self where Self: Sized;
    //fn get(&self, key: &str) -> Option<&String>;
    fn get_hashmap(&self) -> HashMap<String, String> where Self: serde::Serialize {
        let value = serde_json::to_value(self).unwrap_or_default();
        serde_json::from_value::<HashMap<String, String>>(value).unwrap_or_default()
    }
}