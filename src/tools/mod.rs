pub mod error;
pub mod git;
pub mod fs;
pub mod json;
pub mod process;
pub mod collections;
pub mod crypto;
pub mod db;
pub mod string;
pub mod compat;

#[cfg(test)]
pub mod tests;

// Re-exports for convenience
pub use error::{ToolsError, Result};

// Common re-exports from submodules for frequently used functions
pub use fs::{copy_folder, copy_all_files_in_folder, check_path};
pub use git::{get_commit_sha, get_blob_sha};
pub use json::{read_json_file, merge_values, validate_json_by_schema};
pub use process::execute_command;
pub use string::{string_to_path, convert_path_to_absolute, capitalize_first};
pub use db::connect as db_connect;

// Legacy compatibility re-exports
pub use compat::{exit_with_error, LegacyCompat, OptionCompat, CubteraCompat}; 