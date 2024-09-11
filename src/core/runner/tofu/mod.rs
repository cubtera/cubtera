use yansi::Paint;
use crate::prelude::*;
use super::{Runner, RunnerContext};

pub struct TofuRunner {
    ctx: RunnerContext
}

impl TofuRunner {

}

impl Runner for TofuRunner {
    fn new(ctx: RunnerContext) -> Self { TofuRunner { ctx } }
    fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(target: "tofu runner", "Runner is not implemented yet. Waiting for PRs.");
        info!(target: "tofu runner", "Unit: {}", &self.ctx.unit.name.blue());
        info!(target: "tofu runner", "Execute command: {}", &self.ctx.command.join(" ").blue());
        Ok(())
    }
}