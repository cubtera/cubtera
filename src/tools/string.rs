use crate::tools::error::{Result, ToolsError};
use std::path::{Path, PathBuf};

/// Converts a string path to PathBuf with environment variable expansion and tilde expansion
/// 
/// # Arguments
/// * `s` - String path that may contain environment variables and tilde
/// 
/// # Returns
/// * `Result<PathBuf>` - Expanded path or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::string::string_to_path;
/// 
/// let path = string_to_path("~/Documents")?;
/// let path = string_to_path("/$HOME/test")?;
/// let path = string_to_path("/${USER}/project")?;
/// ```
pub fn string_to_path(s: &str) -> Result<PathBuf> {
    if s.is_empty() {
        return Ok(PathBuf::from(""));
    }

    let mut path = s.to_string();

    // Expand tilde to home directory
    if path.starts_with('~') {
        let home = std::env::var("HOME")
            .map_err(|_| ToolsError::path_error("HOME environment variable not found"))?;
        path = path.replacen('~', &home, 1);
    }

    // Expand relative path to absolute
    if path.starts_with("./") {
        let pwd = std::env::current_dir()
            .map_err(|e| ToolsError::path_error(format!("Failed to get current directory: {}", e)))?;
        path = path.replacen(".", pwd.to_str().unwrap_or(""), 1);
    }

    // Expand environment variables in path string
    // Replace ${VAR} and $VAR with actual values
    let with_env = std::env::vars().fold(path, |s, (k, v)| {
        s.replace(&format!("${}", k), &v)
            .replace(&format!("${{{}}}", k), &v)
    });

    Ok(PathBuf::from(with_env))
}

/// Converts a path string to absolute path with tilde and environment variable expansion
/// 
/// # Arguments
/// * `s` - String path to convert
/// 
/// # Returns
/// * `Result<String>` - Absolute path as string or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::string::convert_path_to_absolute;
/// 
/// let abs_path = convert_path_to_absolute("~/Documents".to_string())?;
/// let abs_path = convert_path_to_absolute("./relative/path".to_string())?;
/// ```
pub fn convert_path_to_absolute(s: String) -> Result<String> {
    if s.starts_with('~') {
        let home = std::env::var("HOME")
            .map_err(|_| ToolsError::path_error("HOME environment variable not found"))?;
        Ok(s.replacen('~', &home, 1))
    } else if s.starts_with('.') {
        let pwd = std::env::var("PWD")
            .or_else(|_| std::env::current_dir().map(|p| p.to_string_lossy().to_string()))
            .map_err(|e| ToolsError::path_error(format!("Failed to get current directory: {}", e)))?;
        Ok(s.replacen('.', &pwd, 1))
    } else if s.starts_with('/') {
        Ok(s)
    } else if Path::new(&s).is_relative() {
        let current_dir = std::env::current_dir()
            .map_err(|e| ToolsError::path_error(format!("Failed to get current directory: {}", e)))?;
        Ok(current_dir
            .join(&s)
            .to_str()
            .ok_or_else(|| ToolsError::path_error("Failed to convert path to string"))?
            .to_string())
    } else {
        Ok(s)
    }
}

/// Capitalizes the first character of a string
/// 
/// # Arguments
/// * `s` - String slice to capitalize
/// 
/// # Returns
/// * `String` - String with first character capitalized
/// 
/// # Examples
/// ```
/// use cubtera::tools::string::capitalize_first;
/// 
/// assert_eq!(capitalize_first("hello"), "Hello");
/// assert_eq!(capitalize_first(""), "");
/// assert_eq!(capitalize_first("a"), "A");
/// ```
pub fn capitalize_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

/// Converts a PathBuf to absolute path
/// 
/// # Arguments
/// * `path` - PathBuf to convert
/// 
/// # Returns
/// * `Result<PathBuf>` - Absolute path or error
fn path_to_absolute(path: &PathBuf) -> Result<PathBuf> {
    if path.is_absolute() {
        Ok(path.clone())
    } else {
        let current_dir = std::env::current_dir()
            .map_err(|e| ToolsError::path_error(format!("Failed to get current directory: {}", e)))?;
        Ok(current_dir.join(path))
    }
} 