use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::core::runner::RunnerParams;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
// #[serde(rename_all = "camelCase")]
pub struct BashRunnerParams {
    #[serde(default="default_version")]
    version: String,
    #[serde(default="default_state_backend")]
    state_backend: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    bin_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    extra_params: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    inlet_command: Option<String>,
}

fn default_version() -> String {
    String::from("latest")
}

fn default_state_backend() -> String {
    String::from("s3")
}

impl RunnerParams for BashRunnerParams {
    fn init(params: HashMap<String, String>) -> Self {
        let value = serde_json::to_value(params).unwrap_or_default();
        serde_json::from_value::<BashRunnerParams>(value).unwrap_or_default()
    }
}