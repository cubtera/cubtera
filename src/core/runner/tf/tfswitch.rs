use crate::utils::helper::*;

use rand::Rng;
use std::path::{Path, PathBuf};

pub fn tf_switch(tf_version: &str) -> Result<PathBuf, Box<dyn std::error::Error>> {

    let version = match tf_version {
        "latest" => get_latest(),
            _ => tf_version.into()
    };

    let _ = semver::Version::parse(&version).unwrap_or_exit(format!(
        "Failed to parse tf version {version}. Example: 1.3.5"
    ));

    let path = format!("~/.cubtera/tf/{version}").replace('~', &std::env::var("HOME").unwrap());

    let tf_folder = Path::new(&path).to_path_buf();
    let tf_path = tf_folder.join("terraform");
    if tf_path.is_file() && tf_path.metadata().is_ok() {
        return Ok(tf_path);
    }

    let delay = rand::thread_rng().gen_range(100..800);
    std::thread::sleep(std::time::Duration::from_millis(delay));

    if tf_folder.join("tmp.zip").metadata().is_err() {
        if !tf_folder.exists() {
            std::fs::create_dir_all(tf_folder.clone())?;
        }
        std::fs::File::create(tf_folder.join("tmp.zip"))
            .unwrap_or_exit("Unable to create TF zip file".to_string());
        let os = get_os();
        let url =
            format!("https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os}.zip",);

        log::debug!(target: "", "Downloading TF zip archive: {}", url);

        // ========================================================================
        // workaround for async in sync code
        // let handle = rocket::tokio::runtime::Handle::current();
        // rocket::tokio::task::block_in_place(move || {
        //     handle.block_on(async {
        //         let response = reqwest::get(url)
        //             .await
        //             .unwrap_or_exit("Error downloading TF zip file".to_string());
        //         if !response.status().is_success() {
        //             std::fs::remove_file(Path::new(&path).join("tmp.zip"))
        //                 .unwrap_or_exit("Failed to remove TF zip file".to_string());
        //             exit_with_error(format!("Error downloading TF binary version {}. Status: {}", version, response.status()));
        //         }
        //         let body = response
        //             .bytes()
        //             .await
        //             .unwrap_or_exit("Error downloading TF zip file".to_string());
        //         std::fs::write(Path::new(&path.clone()).join("tmp.zip"), body)
        //             .unwrap_or_exit("Unable to save TF zip file".to_string());
        //     });
        // });
        // ========================================================================
        // Sync implementation of zip file download
        // Async was in use with previous version (keep for a while for ref)
        let response = reqwest::blocking::get(url)
            .unwrap_or_exit("Error downloading TF zip file".to_string());
        if !response.status().is_success() {
            std::fs::remove_file(Path::new(&path).join("tmp.zip"))
                .unwrap_or_exit("Failed to remove TF zip file".to_string());
            exit_with_error(format!("Error downloading TF binary version {}. Status: {}", version, response.status()));
        }
        let body = response.bytes().unwrap_or_else(|_| {
            exit_with_error("Error downloading TF zip file".to_string());
        });
        std::fs::write(Path::new(&path.clone()).join("tmp.zip"), body).unwrap_or_else(|_| {
            exit_with_error("Unable to save TF zip file".to_string());
        });
        // ========================================================================

        log::debug!(target: "", "Unzipping TF from {}", tf_folder.join("tmp.zip").display());
        // Open the downloaded zip file
        let zip_file = std::fs::File::open(tf_folder.join("tmp.zip"))
            .unwrap_or_exit("Failed to open TF zip file".to_string());
        let mut archive = zip::read::ZipArchive::new(zip_file)
            .unwrap_or_exit("Failed to read TF zip file".to_string());
        archive.extract(tf_folder.clone())?;
        std::fs::remove_file(tf_folder.join("tmp.zip"))
            .unwrap_or_exit("Failed to remove TF zip file".to_string())
    }

    while tf_folder.join("tmp.zip").metadata().is_ok() {
        log::debug!(target: "", "{}", format!("Locked by parallel TF zip download. If it's not the case, remove {} manually", tf_folder.join("tmp.zip").display()));
        std::thread::sleep(std::time::Duration::from_millis(500));
    }

    Ok(tf_path)
}

fn get_latest() -> String {
    let resp = reqwest::blocking::get("https://api.releases.hashicorp.com/v1/releases/terraform/latest")
        .unwrap_or_exit("Can't define TF latest version".into())
        .json::<serde_json::Value>()
        .unwrap_or_exit("Can't parse TF version response".into());

    resp["version"].as_str().unwrap_or_exit("Can't parse TF version response".into()).to_string()
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
