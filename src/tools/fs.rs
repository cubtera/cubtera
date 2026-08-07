use crate::tools::error::{Result, ToolsError};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// Copies all files in a folder from source to destination
/// 
/// # Arguments
/// * `src` - Source folder path
/// * `dst` - Destination folder path
/// * `overwrite_existing` - Whether to overwrite existing files
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::copy_all_files_in_folder;
/// use std::path::PathBuf;
/// 
/// copy_all_files_in_folder(
///     PathBuf::from("src_folder"),
///     &PathBuf::from("dst_folder"),
///     true
/// )?;
/// ```
pub fn copy_all_files_in_folder(src: PathBuf, dst: &PathBuf, overwrite_existing: bool) -> Result<()> {
    // Validate source exists
    if !src.exists() {
        return Err(ToolsError::file_not_found(src.to_string_lossy().to_string()));
    }

    if !src.is_dir() {
        return Err(ToolsError::invalid_input(format!("Source path is not a directory: {:?}", src)));
    }

    // Create destination directory if it doesn't exist
    if !dst.exists() {
        std::fs::create_dir_all(dst)
            .map_err(|e| ToolsError::operation_failed(format!("Failed to create directory {:?}: {}", dst, e)))?;
    }

    // Copy files
    for entry in WalkDir::new(&src).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            let src_path = entry.path();
            let file_name = src_path.file_name()
                .ok_or_else(|| ToolsError::operation_failed("Failed to get file name".to_string()))?;
            let dst_path = dst.join(file_name);

            // Skip if file exists and overwrite is disabled
            if dst_path.exists() && !overwrite_existing {
                continue;
            }

            std::fs::copy(src_path, &dst_path)
                .map_err(|e| ToolsError::operation_failed(format!("Failed to copy file {:?} to {:?}: {}", src_path, dst_path, e)))?;
        }
    }

    Ok(())
}

/// Recursively copies a folder and all its contents
/// 
/// # Arguments
/// * `src` - Source folder path
/// * `dst` - Destination folder path
/// * `overwrite_existing` - Whether to overwrite existing files
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::copy_folder;
/// use std::path::PathBuf;
/// 
/// copy_folder(
///     PathBuf::from("src_folder"),
///     &PathBuf::from("dst_folder"),
///     true
/// )?;
/// ```
pub fn copy_folder(src: PathBuf, dst: &PathBuf, overwrite_existing: bool) -> Result<()> {
    // Validate source exists
    if !src.exists() {
        return Err(ToolsError::file_not_found(src.to_string_lossy().to_string()));
    }

    if !src.is_dir() {
        return Err(ToolsError::invalid_input(format!("Source path is not a directory: {:?}", src)));
    }

    // Copy files in current directory
    copy_all_files_in_folder(src.clone(), dst, overwrite_existing)?;

    // Recursively copy subdirectories
    for entry in WalkDir::new(&src).max_depth(1).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_dir() && entry.path() != src {
            let folder_name = entry.path().file_name()
                .ok_or_else(|| ToolsError::operation_failed("Failed to get folder name".to_string()))?;
            let dst_subfolder = dst.join(folder_name);
            
            copy_folder(entry.path().to_path_buf(), &dst_subfolder, overwrite_existing)?;
        }
    }

    Ok(())
}

/// Checks if a path exists and returns it if valid
/// 
/// # Arguments
/// * `path` - Path to check
/// 
/// # Returns
/// * `Result<PathBuf>` - The path if it exists, error otherwise
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::check_path;
/// use std::path::PathBuf;
/// 
/// let valid_path = check_path(PathBuf::from("existing_file.txt"))?;
/// ```
pub fn check_path(path: PathBuf) -> Result<PathBuf> {
    if path.exists() {
        Ok(path)
    } else {
        Err(ToolsError::file_not_found(path.to_string_lossy().to_string()))
    }
}

/// Creates a directory and all parent directories if they don't exist
/// 
/// # Arguments
/// * `path` - Directory path to create
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::create_dir_all;
/// use std::path::PathBuf;
/// 
/// create_dir_all(&PathBuf::from("path/to/new/directory"))?;
/// ```
pub fn create_dir_all(path: &PathBuf) -> Result<()> {
    std::fs::create_dir_all(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to create directory {:?}: {}", path, e)))
}

/// Removes a file
/// 
/// # Arguments
/// * `path` - File path to remove
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::remove_file;
/// use std::path::PathBuf;
/// 
/// remove_file(&PathBuf::from("file_to_delete.txt"))?;
/// ```
pub fn remove_file(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(ToolsError::file_not_found(path.to_string_lossy().to_string()));
    }

    if !path.is_file() {
        return Err(ToolsError::invalid_input(format!("Path is not a file: {:?}", path)));
    }

    std::fs::remove_file(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to remove file {:?}: {}", path, e)))
}

/// Removes a directory and all its contents
/// 
/// # Arguments
/// * `path` - Directory path to remove
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::remove_dir_all;
/// use std::path::PathBuf;
/// 
/// remove_dir_all(&PathBuf::from("directory_to_delete"))?;
/// ```
pub fn remove_dir_all(path: &PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(ToolsError::file_not_found(path.to_string_lossy().to_string()));
    }

    if !path.is_dir() {
        return Err(ToolsError::invalid_input(format!("Path is not a directory: {:?}", path)));
    }

    std::fs::remove_dir_all(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to remove directory {:?}: {}", path, e)))
}

/// Reads the contents of a file as a string
/// 
/// # Arguments
/// * `path` - File path to read
/// 
/// # Returns
/// * `Result<String>` - File contents or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::read_to_string;
/// use std::path::PathBuf;
/// 
/// let contents = read_to_string(&PathBuf::from("file.txt"))?;
/// ```
pub fn read_to_string(path: &PathBuf) -> Result<String> {
    if !path.exists() {
        return Err(ToolsError::file_not_found(path.to_string_lossy().to_string()));
    }

    if !path.is_file() {
        return Err(ToolsError::invalid_input(format!("Path is not a file: {:?}", path)));
    }

    std::fs::read_to_string(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to read file {:?}: {}", path, e)))
}

/// Writes a string to a file
/// 
/// # Arguments
/// * `path` - File path to write to
/// * `contents` - String contents to write
/// 
/// # Returns
/// * `Result<()>` - Success or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::write_string;
/// use std::path::PathBuf;
/// 
/// write_string(&PathBuf::from("output.txt"), "Hello, world!")?;
/// ```
pub fn write_string(path: &PathBuf, contents: &str) -> Result<()> {
    // Create parent directories if they don't exist
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            create_dir_all(&parent.to_path_buf())?;
        }
    }

    std::fs::write(path, contents)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to write file {:?}: {}", path, e)))
}

/// Gets file metadata
/// 
/// # Arguments
/// * `path` - File path to get metadata for
/// 
/// # Returns
/// * `Result<std::fs::Metadata>` - File metadata or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::fs::get_metadata;
/// use std::path::PathBuf;
/// 
/// let metadata = get_metadata(&PathBuf::from("file.txt"))?;
/// let size = metadata.len();
/// ```
pub fn get_metadata(path: &PathBuf) -> Result<std::fs::Metadata> {
    if !path.exists() {
        return Err(ToolsError::file_not_found(path.to_string_lossy().to_string()));
    }

    std::fs::metadata(path)
        .map_err(|e| ToolsError::operation_failed(format!("Failed to get metadata for {:?}: {}", path, e)))
} 