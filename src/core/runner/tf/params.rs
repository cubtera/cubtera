use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use crate::core::runner::RunnerParams;
use crate::prelude::ResultExtUnwrap;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
pub struct TfRunnerParams {
    #[serde(default="default_version")]
    pub version: String,
    #[serde(default="default_state_backend")]
    pub state_backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_params: Option<String>,
    #[serde(default="default_lock_port")]
    pub lock_port: String
}

fn default_lock_port() -> String {
    String::from("65432")
}

fn default_version() -> String {
    String::from("latest")
}

fn default_state_backend() -> String {
    String::from("local")
}

impl TfRunnerParams {
    pub fn get_lock_port(&self) -> u16 {
        self.lock_port.parse().unwrap_or(65432)
    }
}

impl RunnerParams for TfRunnerParams {
    fn init(params: HashMap<String, String>) -> Self {
        let value = serde_json::to_value(params).unwrap_or_exit("Failed to convert runner params".into());
        serde_json::from_value::<TfRunnerParams>(value).unwrap_or_exit("Failed to convert runner params".into())
    }

    // fn get_hashmap(&self) -> HashMap<String, String> {
    //     let value = serde_json::to_value(self).unwrap_or_default();
    //     serde_json::from_value::<HashMap<String, String>>(value).unwrap_or_default()
    // }
}