use crate::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;

/// Centralized logging system for all runners
#[derive(Debug, Clone)]
pub struct RunnerLogger {
    config: LoggingConfig,
    context: LogContext,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub enable_db_logging: bool,
    pub enable_file_logging: bool,
    pub log_file_path: Option<PathBuf>,
    pub log_level: LogLevel,
}

#[derive(Debug, Clone)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogContext {
    pub runner_type: String,
    pub unit_name: String,
    pub command: String,
    pub state_path: String,
    pub dimensions: HashMap<String, String>,
    pub timestamp: u64,
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerLogEntry {
    pub context: LogContext,
    pub level: String,
    pub message: String,
    pub details: Option<Value>,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

impl RunnerLogger {
    pub fn new(runner_type: &str, unit: &Unit, command: &[String]) -> Self {
        let config = LoggingConfig::from_global_config();
        let context = LogContext::new(runner_type, unit, command);

        Self { config, context }
    }

    /// Log runner start
    pub fn log_start(&self) {
        let entry = RunnerLogEntry {
            context: self.context.clone(),
            level: "INFO".to_string(),
            message: format!(
                "Starting {} runner for unit {}",
                self.context.runner_type, self.context.unit_name
            ),
            details: Some(json!({
                "command": self.context.command,
                "dimensions": self.context.dimensions
            })),
            exit_code: None,
            duration_ms: None,
            error: None,
        };

        self.write_log(&entry);
    }

    /// Log runner completion
    pub fn log_completion(&self, exit_code: i32, duration_ms: u64, error: Option<String>) {
        let level = if exit_code == 0 { "INFO" } else { "ERROR" };
        let message = if exit_code == 0 {
            format!("Successfully completed {} runner", self.context.runner_type)
        } else {
            format!(
                "Failed {} runner with exit code {}",
                self.context.runner_type, exit_code
            )
        };

        let entry = RunnerLogEntry {
            context: self.context.clone(),
            level: level.to_string(),
            message,
            details: Some(json!({
                "execution_time_ms": duration_ms,
                "success": exit_code == 0
            })),
            exit_code: Some(exit_code),
            duration_ms: Some(duration_ms),
            error,
        };

        self.write_log(&entry);
    }

    /// Log runner step
    pub fn log_step(&self, step: &str, message: &str, details: Option<Value>) {
        let entry = RunnerLogEntry {
            context: self.context.clone(),
            level: "INFO".to_string(),
            message: format!("[{}] {}", step.to_uppercase(), message),
            details,
            exit_code: None,
            duration_ms: None,
            error: None,
        };

        self.write_log(&entry);
    }

    /// Log runner error
    pub fn log_error(&self, step: &str, error: &str, details: Option<Value>) {
        let entry = RunnerLogEntry {
            context: self.context.clone(),
            level: "ERROR".to_string(),
            message: format!("[{}] {}", step.to_uppercase(), error),
            details,
            exit_code: None,
            duration_ms: None,
            error: Some(error.to_string()),
        };

        self.write_log(&entry);
    }

    /// Write log entry to configured destinations
    fn write_log(&self, entry: &RunnerLogEntry) {
        // Log to standard logger
        match entry.level.as_str() {
            "DEBUG" => debug!(target: "runner", "{}", entry.message),
            "INFO" => info!(target: "runner", "{}", entry.message),
            "WARN" => warn!(target: "runner", "{}", entry.message),
            "ERROR" => error!(target: "runner", "{}", entry.message),
            _ => info!(target: "runner", "{}", entry.message),
        }

        // Write to database if enabled
        if self.config.enable_db_logging {
            if let Err(e) = self.write_to_db(entry) {
                warn!(target: "runner", "Failed to write to database: {}", e);
            }
        }

        // Write to file if enabled
        if self.config.enable_file_logging {
            if let Err(e) = self.write_to_file(entry) {
                warn!(target: "runner", "Failed to write to file: {}", e);
            }
        }
    }

    /// Write log entry to database
    fn write_to_db(&self, entry: &RunnerLogEntry) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(client) = &GLOBAL_CFG.db_client {
            let db = client.database(&GLOBAL_CFG.org);
            let collection = db.collection::<mongodb::bson::Bson>("runner_logs");

            let doc = mongodb::bson::to_bson(&entry)?;
            collection.insert_one(doc).run()?;
        }
        Ok(())
    }

    /// Write log entry to file
    fn write_to_file(&self, entry: &RunnerLogEntry) -> Result<(), Box<dyn std::error::Error>> {
        use std::fs::OpenOptions;
        use std::io::Write;

        let log_path = self
            .config
            .log_file_path
            .as_ref()
            .map(|p| p.clone())
            .unwrap_or_else(|| {
                PathBuf::from(&GLOBAL_CFG.temp_folder_path)
                    .join("logs")
                    .join(format!("runner-{}.log", &self.context.session_id))
            });

        // Ensure log directory exists
        if let Some(parent) = log_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)?;

        let log_line = format!(
            "[{}] [{}] [{}] {}\n",
            chrono::DateTime::from_timestamp(self.context.timestamp as i64, 0)
                .unwrap_or_default()
                .format("%Y-%m-%d %H:%M:%S"),
            entry.level,
            self.context.runner_type,
            serde_json::to_string(entry)?
        );

        file.write_all(log_line.as_bytes())?;
        file.flush()?;

        Ok(())
    }
}

impl LoggingConfig {
    fn from_global_config() -> Self {
        Self {
            enable_db_logging: GLOBAL_CFG.db_client.is_some(),
            enable_file_logging: true, // Always enable file logging as fallback
            log_file_path: None,       // Use default path
            log_level: LogLevel::Info,
        }
    }
}

impl LogContext {
    fn new(runner_type: &str, unit: &Unit, command: &[String]) -> Self {
        let state_path = unit.get_unit_state_path();
        let dimensions: HashMap<String, String> = state_path
            .split('/')
            .filter_map(|dim| {
                let parts: Vec<&str> = dim.split(':').collect();
                if parts.len() == 2 {
                    Some((parts[0].to_string(), parts[1].to_string()))
                } else {
                    None
                }
            })
            .collect();

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let session_id = format!("{}-{}", unit.get_name(), timestamp);

        Self {
            runner_type: runner_type.to_string(),
            unit_name: unit.get_name(),
            command: command.join(" "),
            state_path,
            dimensions,
            timestamp,
            session_id,
        }
    }
}

/// Extension trait for Unit to provide logging capabilities
pub trait UnitLogging {
    fn create_logger(&self, runner_type: &str, command: &[String]) -> RunnerLogger;
}

impl UnitLogging for Unit {
    fn create_logger(&self, runner_type: &str, command: &[String]) -> RunnerLogger {
        RunnerLogger::new(runner_type, self, command)
    }
}
