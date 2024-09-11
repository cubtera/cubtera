use std::error::Error;
use std::process::Command;

use crate::prelude::*;
use super::{Runner, RunnerContext};
use yansi::Paint;

mod params;
use params::BashRunnerParams;
use super::RunnerParams;

pub struct BashRunner {
    ctx: RunnerContext,
    params: BashRunnerParams
}

impl Runner for BashRunner {
    fn new(ctx: RunnerContext) -> Self {
        let params = BashRunnerParams::init(ctx.params.clone());
        BashRunner {
            ctx,
            params
        }
    }

    fn copy_files(&self) -> Result<(), Box<dyn Error>> {
        info!(target: "bash runner", "Copy files to: {}", &self.ctx.unit.temp_folder.to_string_lossy().blue());
        self.ctx.unit.remove_temp_folder();
        self.ctx.unit.copy_files();

        Ok(())
    }

    fn run(&self) -> Result<(), Box<dyn Error>> {
        let params = self.params.get_hashmap();

        let mut args = self.ctx.command.clone();
        params.get("extra_params").map(|s| args.push(s.clone()));
        // args.reverse();

        let binary = params.get("bin_path")
            .unwrap_or_exit("Runner binary path is not configured".into());

        info!(target: "bash runner", "Execute command: {} {}", &binary.blue(), &args.join(" ").blue());

        let mut command = Command::new(&binary);
        let mut child = command
            .current_dir(self.ctx.unit.temp_folder.to_str().unwrap())
            .args(&args)
            .spawn()
            .unwrap_or_exit(format!(
                "Failed to run: {} {}",
                binary.red(), args.join(" ").red()
            ));
        let result = child
            .wait()
            .unwrap_or_exit("Failed to get bash runner exitcode".to_string());
        debug!(target: "bash runner", "Cubtera finished with code: {}", result );

        let exit_code = result.code().unwrap_or(1);

        std::process::exit(exit_code)
    }
}