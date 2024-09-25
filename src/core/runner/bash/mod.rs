use std::error::Error;
use std::process::Command;
use crate::prelude::*;
use super::{Runner, RunnerLoad};
use yansi::Paint;
use serde_json::{json, Value};

mod params;
use params::BashRunnerParams;
use super::RunnerParams;

pub struct BashRunner {
    load: RunnerLoad,
    params: BashRunnerParams,
    ctx: Value
}

impl Runner for BashRunner {
    fn new(load: RunnerLoad) -> Self {
        let params = BashRunnerParams::init(load.params.clone());
        let ctx = Value::Object(serde_json::Map::new());
        BashRunner {
            load,
            params,
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

    // fn copy_files(&self) -> Result<(), Box<dyn Error>> {
    //     info!(target: "bash runner", "Copy files to: {}", &self.ctx.unit.temp_folder.to_string_lossy().blue());
    //     self.ctx.unit.remove_temp_folder();
    //     self.ctx.unit.copy_files();
    //
    //     Ok(())
    // }

    fn runner(&mut self) -> Result<(), Box<dyn Error>> {
        let params = self.params.get_hashmap();

        let mut args = self.load.command.clone();
        params.get("extra_params").map(|s| args.push(s.clone()));
        // args.reverse();

        let binary = params.get("bin_path")
            .unwrap_or_exit("Runner binary path is not configured".into());

        info!(target: "bash runner", "Execute command: {} {}", &binary.blue(), &args.join(" ").blue());

        let mut command = Command::new(&binary);
        let mut child = command
            .current_dir(self.load.unit.temp_folder.to_str().unwrap())
            .args(&args)
            .spawn()
            .unwrap_or_exit(format!(
                "Failed to run: {} {}",
                binary.red(), args.join(" ").red()
            ));
        let result = child
            .wait()
            .unwrap_or_exit("Failed to get bash runner exitcode".to_string());
        // debug!(target: "bash runner", "Cubtera finished with code: {}", result );

        let exit_code = result.code().unwrap_or(1);

        // std::process::exit(exit_code)
        self.update_ctx("exit_code", json!(exit_code));
        Ok(())
    }
}