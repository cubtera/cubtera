use crate::tools::error::{Result, ToolsError};
use std::path::PathBuf;

/// Gets the blob SHA for a file at the given path in the git repository
/// 
/// # Arguments
/// * `path` - PathBuf to the file in the repository
/// 
/// # Returns
/// * `Result<String>` - SHA of the blob or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::git::get_blob_sha;
/// use std::path::PathBuf;
/// 
/// let sha = get_blob_sha(&PathBuf::from("src/main.rs"))?;
/// ```
pub fn get_blob_sha(path: &PathBuf) -> Result<String> {
    let abs_path = path_to_absolute(path)?;

    // Open the repository
    let repo = git2::Repository::discover(&abs_path)?;

    // Get the repository's workdir (root directory)
    let repo_root = repo.workdir()
        .ok_or_else(|| ToolsError::Git(git2::Error::from_str("Could not get repository working directory")))?;

    // Convert the absolute path to a path relative to the repository root
    let relative_path = abs_path.strip_prefix(repo_root)
        .map_err(|_| ToolsError::Git(git2::Error::from_str("Failed to get relative path")))?;

    // Get the HEAD commit
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;
    
    // Get the tree from the commit
    let tree = commit.tree()?;
    
    // Get the tree entry for the relative_path
    let entry = tree.get_path(relative_path)?;

    // Get the object ID (SHA) of the entry
    let entry_sha = entry.id().to_string();

    Ok(entry_sha)
}

/// Gets the commit SHA for the repository containing the given path
/// 
/// # Arguments
/// * `path` - PathBuf to any file/directory in the repository
/// 
/// # Returns
/// * `Result<String>` - SHA of the HEAD commit or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::git::get_commit_sha;
/// use std::path::PathBuf;
/// 
/// let sha = get_commit_sha(&PathBuf::from("."))?;
/// ```
pub fn get_commit_sha(path: &PathBuf) -> Result<String> {
    let abs_path = path_to_absolute(path)?;

    // Open the repository
    let repo = git2::Repository::discover(abs_path)?;

    // Get the HEAD commit
    let head = repo.head()?;
    let commit = head.peel_to_commit()?;

    // Get the commit ID (SHA)
    let commit_sha = commit.id().to_string();

    Ok(commit_sha)
}

/// Gets the current branch name for the repository containing the given path
/// 
/// # Arguments
/// * `path` - PathBuf to any file/directory in the repository
/// 
/// # Returns
/// * `Result<String>` - Name of the current branch or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::git::get_current_branch;
/// use std::path::PathBuf;
/// 
/// let branch = get_current_branch(&PathBuf::from("."))?;
/// ```
pub fn get_current_branch(path: &PathBuf) -> Result<String> {
    let abs_path = path_to_absolute(path)?;
    let repo = git2::Repository::discover(abs_path)?;
    
    let head = repo.head()?;
    let branch_name = head.shorthand()
        .ok_or_else(|| ToolsError::Git(git2::Error::from_str("Failed to get branch name")))?;
    
    Ok(branch_name.to_string())
}

/// Checks if the repository at the given path has uncommitted changes
/// 
/// # Arguments
/// * `path` - PathBuf to any file/directory in the repository
/// 
/// # Returns
/// * `Result<bool>` - true if there are uncommitted changes, false otherwise
/// 
/// # Examples
/// ```
/// use cubtera::tools::git::has_uncommitted_changes;
/// use std::path::PathBuf;
/// 
/// let has_changes = has_uncommitted_changes(&PathBuf::from("."))?;
/// ```
pub fn has_uncommitted_changes(path: &PathBuf) -> Result<bool> {
    let abs_path = path_to_absolute(path)?;
    let repo = git2::Repository::discover(abs_path)?;
    
    let mut status_options = git2::StatusOptions::new();
    status_options.include_untracked(true);
    
    let statuses = repo.statuses(Some(&mut status_options))?;
    
    Ok(!statuses.is_empty())
}

/// Gets the remote URL for the repository containing the given path
/// 
/// # Arguments
/// * `path` - PathBuf to any file/directory in the repository
/// * `remote_name` - Name of the remote (default: "origin")
/// 
/// # Returns
/// * `Result<String>` - URL of the remote or error
/// 
/// # Examples
/// ```
/// use cubtera::tools::git::get_remote_url;
/// use std::path::PathBuf;
/// 
/// let url = get_remote_url(&PathBuf::from("."), "origin")?;
/// ```
pub fn get_remote_url(path: &PathBuf, remote_name: &str) -> Result<String> {
    let abs_path = path_to_absolute(path)?;
    let repo = git2::Repository::discover(abs_path)?;
    
    let remote = repo.find_remote(remote_name)?;
    let url = remote.url()
        .ok_or_else(|| ToolsError::Git(git2::Error::from_str("Remote URL not found")))?;
    
    Ok(url.to_string())
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