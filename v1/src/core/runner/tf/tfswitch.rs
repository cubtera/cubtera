use std::net::TcpListener;
use std::ops::{Add, Not, Rem};
use crate::utils::helper::*;

use rand::Rng;
use std::path::{Path, PathBuf};
use log::{debug, info};

pub fn tf_switch(tf_version: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {
    let version = match tf_version {
        "latest" => get_latest(),
        _ => tf_version.into(),
    };

    let _ = semver::Version::parse(&version).unwrap_or_exit(format!(
        "Failed to parse tf version {version}. Use semver format.",
    ));

    let path = format!("~/.cubtera/tf/{version}").replace('~', &std::env::var("HOME").unwrap());

    let tf_folder = Path::new(&path).to_path_buf();
    let tf_path = tf_folder.join("terraform");

    // take random delay to avoid parallel downloads
    let delay = rand::rng().random_range(100..800);
    std::thread::sleep(std::time::Duration::from_millis(delay));

    loop {
        match is_file_available(&tf_path) {
            true => return Ok(tf_path),
            false => rock_n_roll(&version, &tf_folder)?
        }
    }
}

fn rock_n_roll(version: &str, tf_folder: &PathBuf ) -> Result<(), Box<dyn std::error::Error>> {
    let port = version.replace('.', "").parse::<u16>()
        .unwrap_or_default()
        .rem(5430).add(60000);

    match acquire_lock(port) {
        Ok(_lock) => download_tf(&tf_folder, &version)?,
        Err(_) => wait_for_lock(port)?
    }
    Ok(())
}

fn is_file_available(path: &PathBuf) -> bool {
    // Check if file exists
    path.exists().not().then(|| return false);

    // Check if file is readable
    std::fs::metadata(&path).is_ok_and(|m| m.len() == 0).then(|| return false);

    // Check if binary is functional
    std::process::Command::new(&path)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .spawn()
        .and_then(|mut proc| proc.wait())
        .is_ok()
}

fn acquire_lock(port: u16) -> std::io::Result<TcpListener> {
    TcpListener::bind(("0.0.0.0", port))
}

fn wait_for_lock(port: u16) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    info!(target: "tf switch", "Waiting for terraform download in parallel, port {port} locked");
    while start.elapsed() < std::time::Duration::from_secs(120) {
        match TcpListener::bind(("0.0.0.0", port)) {
            Ok(_) => return Ok(()),
            Err(_) => std::thread::sleep(std::time::Duration::from_millis(1000)),
        }
    }
    Err(Box::from(format!("Timeout waiting for terraform download lock on port {port}")))
}

fn download_tf(tf_folder: &PathBuf, version: &str) -> Result<(), Box<dyn std::error::Error>> {
    if !tf_folder.exists() {
        std::fs::create_dir_all(tf_folder.clone())?;
    }
    std::fs::File::create(tf_folder.join("tmp.zip"))?;

    let os = get_os();
    let url = format!(
        "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}.zip",
    );

    debug!(target: "", "Downloading TF zip archive: {}", url);

    let response =
        reqwest::blocking::get(url).unwrap_or_exit("Error downloading TF zip file".into());

    if !response.status().is_success() {

        std::fs::remove_file(tf_folder.join("tmp.zip"))?;

        exit_with_error(format!(
            "Error downloading TF binary version {}. Status: {}",
            version,
            response.status()
        ));
    }

    let body = response.bytes().unwrap_or_else(|_| {
        exit_with_error("Error downloading TF zip file".to_string());
    });

    std::fs::write(tf_folder.join("tmp.zip"), body).unwrap_or_else(|_| {
        exit_with_error("Unable to save TF zip file".to_string());
    });

    debug!(target: "", "Unzipping TF from {}", tf_folder.join("tmp.zip").display());

    // Open the downloaded zip file
    let zip_file = std::fs::File::open(tf_folder.join("tmp.zip"))
        .unwrap_or_exit("Failed to open TF zip file".to_string());

    let mut archive = zip::read::ZipArchive::new(zip_file)
        .unwrap_or_exit("Failed to read TF zip file".to_string());

    archive.extract(tf_folder.clone())?;

    std::fs::remove_file(tf_folder.join("tmp.zip"))
        .unwrap_or_exit("Failed to remove TF zip file".to_string());

    Ok(())
}

fn get_latest() -> String {
    let resp =
        reqwest::blocking::get("https://api.releases.hashicorp.com/v1/releases/terraform/latest")
            .unwrap_or_exit("Can't define TF latest version".into())
            .json::<serde_json::Value>()
            .unwrap_or_exit("Can't parse TF version response".into());

    resp["version"]
        .as_str()
        .unwrap_or_exit("Can't parse TF version response".into())
        .to_string()
}

fn get_os() -> String {
    format!("{}_{}", get_os_type(), get_os_architecture())
}

fn get_os_type<'a>() -> &'a str {
    let os = std::env::consts::OS;
    // Possible values:
    // windows / linux / macos / ios / freebsd / dragonfly / netbsd / openbsd / solaris / android
    match os {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        _ => panic!("Unsupported OS: {}", os),
    }
}

fn get_os_architecture<'a>() -> &'a str {
    let arch = std::env::consts::ARCH;
    // Possible values:
    // x86 / x86_64 / aarch64 / arm / mips / mips64 / powerpc / powerpc64 / riscv64 / s390x / sparc64
    match arch {
        "x86" => "386",
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        _ => panic!("Unsupported OS architecture: {}", arch),
    }
}
