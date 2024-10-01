use crate::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct RunnerParams {
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default = "default_state_backend")]
    pub state_backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_args: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inlet_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outlet_command: Option<String>,
    #[serde(default = "default_lock_port")]
    pub lock_port: String,
}

#[allow(dead_code)]
impl RunnerParams {
    pub fn init(params: HashMap<String, String>) -> Self {
        let value =
            serde_json::to_value(params).unwrap_or_exit("Failed to convert runner params".into());
        serde_json::from_value::<RunnerParams>(value)
            .unwrap_or_exit("Failed to convert runner params".into())
    }

    pub fn get_params_hashmap(&self) -> HashMap<String, String> {
        let value = serde_json::to_value(self).unwrap_or_default();
        serde_json::from_value::<HashMap<String, String>>(value).unwrap_or_default()
    }

    pub fn get_lock_port(&self) -> u16 {
        self.lock_port.parse().unwrap_or(65432)
    }

    pub fn get_version(&self) -> String {
        self.version.clone()
    }

    pub fn get_state_backend(&self) -> String {
        self.state_backend.clone()
    }
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
