use serde_json::Value;
use std::error::Error;

use crate::prelude::*;

use super::tf::TfRunner;
use super::{Runner, RunnerLoad};

pub struct TofuRunner {
    inner: TfRunner,
}

impl TofuRunner {}

impl Runner for TofuRunner {
    fn new(load: RunnerLoad) -> Self {
        // let ctx = Value::Object(serde_json::Map::new());

        match load.params.runner_command {
            Some(_) => TofuRunner {
                inner: TfRunner::new(load),
            },
            _ => exit_with_error("OpenTofu runner command is not defined".to_string()),
        }
    }

    fn get_load(&self) -> &RunnerLoad {
        self.inner.get_load()
    }

    fn get_ctx(&self) -> &Value {
        self.inner.get_ctx()
    }

    fn get_ctx_mut(&mut self) -> &mut Value {
        self.inner.get_ctx_mut()
    }

    fn copy_files(&mut self) -> Result<(), Box<dyn Error>> {
        self.inner.copy_files()
    }

    fn change_files(&mut self) -> Result<(), Box<dyn Error>> {
        self.inner.change_files()
    }

    fn run(&mut self) -> Result<Value, Box<dyn std::error::Error>> {
        self.inner.run()
    }
}
