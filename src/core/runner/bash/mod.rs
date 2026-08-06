// Simple bash runner implementation
// Using default Runner Trait implementation's methods

use super::{Runner, RunnerLoad};
use crate::prelude::*;
use log::{debug, info};
use serde_json::{json, Value};

pub struct BashRunner {
    load: RunnerLoad,
    ctx: Value,
}

impl Runner for BashRunner {
    fn new(load: RunnerLoad) -> Self {
        let ctx = Value::Object(serde_json::Map::new());
        BashRunner { load, ctx }
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

    fn logger(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        debug!(target: "bash runner", "Logger method.");

        if GLOBAL_CFG.dlog_db.is_some() {
            let exit_code = self.ctx.get("runner_exit_code")
                .and_then(|v| v.as_i64())
                .unwrap_or(0) as i32;

            let command = self.load.command.first()
                .map(|s| s.as_str())
                .unwrap_or("");

            let dlog = Dlog::build(
                self.load.unit.clone(),
                command.into(),
                exit_code
            );
            let _ = dlog
                .put(&GLOBAL_CFG.org)
                .check_with_warn("Can't put dlog to DB");
            info!(target: "bash runner", "Dlog data was saved");
        }

        self.update_ctx("logger", json!("executed"));
        debug!(target: "bash runner", "Final context: {}", self.ctx.to_string());

        Ok(())
    }
}
