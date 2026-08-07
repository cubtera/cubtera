use crate::tools::error::{Result, ToolsError};
use crate::tools::string::string_to_path;
use std::collections::HashMap;
use std::process::{Command, ExitStatus};

/// Executes a command with environment variables
/// 
/// # Arguments
/// * `command` - Command string to execute
/// * `current_dir` - Working directory for the command
/// * `env_vars` - Environment variables to set
/// 
/// # Returns
/// * `Result<ExitStatus>` - Exit status or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::process::execute_command;
/// use std::collections::HashMap;
/// 
/// let mut env_vars = HashMap::new();
/// env_vars.insert("MY_VAR".to_string(), "value".to_string());
/// 
/// let status = execute_command("echo $MY_VAR", "/tmp", env_vars)?;
/// ```
pub fn execute_command(
    command: &str,
    current_dir: &str,
    env_vars: HashMap<String, String>,
) -> Result<ExitStatus> {
    if command.is_empty() {
        return Err(ToolsError::invalid_input("Command cannot be empty".to_string()));
    }

    let mut command_parts = command.split_whitespace();
    let binary = command_parts.next()
        .ok_or_else(|| ToolsError::invalid_input("Command is empty".to_string()))?;
    
    let path = string_to_path(binary)?;
    let args: Vec<&str> = command_parts.collect();

    let mut process = Command::new(path)
        .current_dir(current_dir)
        .args(args)
        .envs(env_vars)
        .spawn()
        .map_err(|e| ToolsError::process_error(format!("Failed to spawn process: {}", e)))?;

    let status = process.wait()
        .map_err(|e| ToolsError::process_error(format!("Failed to wait for process: {}", e)))?;

    Ok(status)
}

// TODO: Add more process operations as needed
// - execute_command_with_output
// - execute_command_async
// - kill_process
// - process_exists
// etc. 