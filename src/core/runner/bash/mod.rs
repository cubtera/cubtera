// Simple bash runner implementation
// Using default Runner Trait implementation's methods

use super::{Runner, RunnerLoad};
use serde_json::Value;


pub struct BashRunner {
    load: RunnerLoad,
    ctx: Value
}

impl Runner for BashRunner {
    fn new(load: RunnerLoad) -> Self {
        let ctx = Value::Object(serde_json::Map::new());
        BashRunner {
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
}