//! Terraform version switcher
//!
//! Downloads and caches terraform binaries by version.
//! Uses port-based locking to prevent parallel downloads.

use cubtera_core::error::{AppError, AppResult};
use std::net::TcpListener;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Default cubtera home directory
fn cubtera_home() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(home).join(".cubtera")
}

/// Get the terraform binary path for a specific version.
/// Downloads the binary if not already cached.
pub fn tf_switch(version: &str) -> AppResult<PathBuf> {
    let version = if version == "latest" {
        get_latest_version()?
    } else {
        version.to_string()
    };

    // Validate semver format
    semver::Version::parse(&version).map_err(|_| {
        AppError::runner(format!(
            "Invalid terraform version '{}'. Use semver format (e.g., 1.6.6)",
            version
        ))
    })?;

    let tf_folder = cubtera_home().join("tf").join(&version);
    let tf_path = tf_folder.join("terraform");

    // Random delay to reduce lock contention
    let delay = rand::random::<u64>() % 700 + 100;
    std::thread::sleep(Duration::from_millis(delay));

    // Try to get existing binary or download
    loop {
        if is_binary_available(&tf_path) {
            return Ok(tf_path);
        }
        acquire_and_download(&version, &tf_folder)?;
    }
}

/// Check if the terraform binary exists and is functional
fn is_binary_available(path: &PathBuf) -> bool {
    if !path.exists() {
        return false;
    }

    // Check file is not empty (incomplete download)
    if let Ok(meta) = std::fs::metadata(path) {
        if meta.len() == 0 {
            return false;
        }
    }

    // Test if binary actually works
    std::process::Command::new(path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .and_then(|mut proc| proc.wait())
        .is_ok()
}

/// Calculate lock port from version string
fn version_to_port(version: &str) -> u16 {
    let num: u32 = version
        .replace('.', "")
        .parse()
        .unwrap_or(0);
    ((num % 5430) + 60000) as u16
}

/// Try to acquire lock and download terraform
fn acquire_and_download(version: &str, tf_folder: &PathBuf) -> AppResult<()> {
    let port = version_to_port(version);

    match TcpListener::bind(("127.0.0.1", port)) {
        Ok(_lock) => {
            // We got the lock, download terraform
            info!("Downloading Terraform {}...", version);
            download_terraform(tf_folder, version)?;
            // Lock is released when _lock goes out of scope
        }
        Err(_) => {
            // Another process is downloading, wait
            wait_for_download(port, version)?;
        }
    }
    Ok(())
}

/// Wait for another process to finish downloading
fn wait_for_download(port: u16, version: &str) -> AppResult<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(120);

    info!(
        "Waiting for terraform {} download (port {} locked)...",
        version, port
    );

    while start.elapsed() < timeout {
        // Try to acquire the lock
        if TcpListener::bind(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_secs(1));
    }

    Err(AppError::runner(format!(
        "Timeout waiting for terraform {} download (port {})",
        version, port
    )))
}

/// Download and extract terraform binary
fn download_terraform(tf_folder: &PathBuf, version: &str) -> AppResult<()> {
    // Create directory
    std::fs::create_dir_all(tf_folder).map_err(|e| {
        AppError::runner(format!("Failed to create terraform directory: {}", e))
    })?;

    let zip_path = tf_folder.join("tmp.zip");
    let tf_path = tf_folder.join("terraform");

    // Create placeholder to prevent other processes from downloading
    std::fs::File::create(&zip_path).map_err(|e| {
        AppError::runner(format!("Failed to create temp file: {}", e))
    })?;

    let os = get_os_string();
    let url = format!(
        "https://releases.hashicorp.com/terraform/{}/terraform_{}_{}.zip",
        version, version, os
    );

    debug!("Downloading: {}", url);

    // Download zip
    let response = reqwest::blocking::get(&url).map_err(|e| {
        let _ = std::fs::remove_file(&zip_path);
        AppError::runner(format!("Failed to download terraform: {}", e))
    })?;

    if !response.status().is_success() {
        let _ = std::fs::remove_file(&zip_path);
        return Err(AppError::runner(format!(
            "Failed to download terraform {}: HTTP {}",
            version,
            response.status()
        )));
    }

    let bytes = response.bytes().map_err(|e| {
        let _ = std::fs::remove_file(&zip_path);
        AppError::runner(format!("Failed to read download: {}", e))
    })?;

    std::fs::write(&zip_path, &bytes).map_err(|e| {
        AppError::runner(format!("Failed to save zip: {}", e))
    })?;

    // Extract zip
    debug!("Extracting: {}", zip_path.display());
    let zip_file = std::fs::File::open(&zip_path).map_err(|e| {
        AppError::runner(format!("Failed to open zip: {}", e))
    })?;

    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| {
        AppError::runner(format!("Failed to read zip: {}", e))
    })?;

    archive.extract(tf_folder).map_err(|e| {
        AppError::runner(format!("Failed to extract zip: {}", e))
    })?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&tf_path)
            .map_err(|e| AppError::runner(format!("Failed to get permissions: {}", e)))?
            .permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&tf_path, perms)
            .map_err(|e| AppError::runner(format!("Failed to set permissions: {}", e)))?;
    }

    // Cleanup zip file
    let _ = std::fs::remove_file(&zip_path);

    info!("Terraform {} installed at {}", version, tf_path.display());
    Ok(())
}

/// Get latest terraform version from HashiCorp API
fn get_latest_version() -> AppResult<String> {
    let url = "https://api.releases.hashicorp.com/v1/releases/terraform/latest";
    
    let response: serde_json::Value = reqwest::blocking::get(url)
        .map_err(|e| AppError::runner(format!("Failed to fetch latest version: {}", e)))?
        .json()
        .map_err(|e| AppError::runner(format!("Failed to parse version response: {}", e)))?;

    response["version"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| AppError::runner("Failed to get version from response".to_string()))
}

/// Get OS string for download URL (e.g., "darwin_arm64", "linux_amd64")
fn get_os_string() -> String {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => panic!("Unsupported OS: {}", other),
    };

    let arch = match std::env::consts::ARCH {
        "x86" => "386",
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => panic!("Unsupported architecture: {}", other),
    };

    format!("{}_{}", os, arch)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_to_port() {
        // 1.6.6 -> 166 % 5430 + 60000 = 60166
        assert_eq!(version_to_port("1.6.6"), 60166);
        // 1.5.0 -> 150 % 5430 + 60000 = 60150
        assert_eq!(version_to_port("1.5.0"), 60150);
    }

    #[test]
    fn test_get_os_string() {
        let os = get_os_string();
        assert!(os.contains('_'));
        // Should be something like "darwin_arm64" or "linux_amd64"
    }
}

