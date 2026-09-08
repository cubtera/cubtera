//! Binary version resolution, parameterized per `TfLikeRunner` instance.
//!
//! v2 had exactly one strategy here (`tfswitch`, hard-baked into
//! `TerraformRunner`, guarded by a `TcpListener` bound on a fixed port -
//! the "лочится только init через TCP-порт" pattern §6/§9 call out as a
//! locking anti-pattern to retire) and OpenTofu had none at all - it just
//! shelled out to whatever `tofu` happened to be on `PATH`, silently. Here
//! both are real [`VersionResolver`] implementations: [`PathVersionResolver`]
//! for "no pinning, but verify what's there" and [`TfSwitchResolver`] for
//! "download and cache a pinned version", locked with a plain advisory
//! lockfile instead of a TCP port (works across machines sharing the cache
//! directory, doesn't exhaust ports, and self-heals from a crashed holder
//! instead of hanging forever).

use crate::error::{ExecError, ExecResult};
use async_trait::async_trait;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Resolves the concrete binary path to run for a (possibly absent)
/// requested version.
#[async_trait]
pub trait VersionResolver: Send + Sync {
    async fn resolve(&self, requested: Option<&str>) -> ExecResult<PathBuf>;
}

/// Resolves a binary from `PATH` - no download, no cache. Correct for
/// runners without a pinning story (OpenTofu today). Still a real
/// implementation, not a stub: it verifies the binary actually runs and,
/// when a version was requested, that `<binary> version`'s output
/// mentions it - a mismatch is a hard [`ExecError::Version`], never a
/// silent ignore.
pub struct PathVersionResolver {
    binary_name: String,
}

impl PathVersionResolver {
    pub fn new(binary_name: impl Into<String>) -> Self {
        Self {
            binary_name: binary_name.into(),
        }
    }
}

#[async_trait]
impl VersionResolver for PathVersionResolver {
    async fn resolve(&self, requested: Option<&str>) -> ExecResult<PathBuf> {
        let output = tokio::process::Command::new(&self.binary_name)
            .arg("version")
            .output()
            .await
            .map_err(|e| {
                ExecError::Version(format!("{} not found on PATH: {e}", self.binary_name))
            })?;

        if let Some(want) = requested {
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.contains(want) {
                return Err(ExecError::Version(format!(
                    "{} on PATH does not match requested version {want:?} (reported: {})",
                    self.binary_name,
                    text.lines().next().unwrap_or("<no output>"),
                )));
            }
        }
        Ok(PathBuf::from(&self.binary_name))
    }
}

/// Downloads and caches pinned Terraform releases under `cache_dir/<version>/terraform`.
pub struct TfSwitchResolver {
    cache_dir: PathBuf,
    lock_timeout: Duration,
}

impl TfSwitchResolver {
    pub fn new(cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            cache_dir: cache_dir.into(),
            lock_timeout: Duration::from_secs(120),
        }
    }

    /// Override the lock-reclaim timeout (tests only need this to avoid a
    /// two-minute wait).
    pub fn with_lock_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }
}

#[async_trait]
impl VersionResolver for TfSwitchResolver {
    async fn resolve(&self, requested: Option<&str>) -> ExecResult<PathBuf> {
        let version = match requested {
            Some(v) if v != "latest" => v.to_string(),
            _ => fetch_latest_version().await?,
        };
        semver::Version::parse(&version).map_err(|_| {
            ExecError::Version(format!(
                "invalid terraform version {version:?}; use semver, e.g. 1.9.0"
            ))
        })?;

        let version_dir = self.cache_dir.join(&version);
        let binary_path = version_dir.join("terraform");

        if is_binary_available(&binary_path).await {
            return Ok(binary_path);
        }

        tokio::fs::create_dir_all(&self.cache_dir).await?;
        let lock_path = self.cache_dir.join(format!(".{version}.lock"));
        acquire_file_lock(&lock_path, self.lock_timeout).await?;

        // Another process may have finished downloading while we waited.
        let result = if is_binary_available(&binary_path).await {
            Ok(binary_path.clone())
        } else {
            download_terraform(&version_dir, &version)
                .await
                .map(|()| binary_path.clone())
        };
        let _ = tokio::fs::remove_file(&lock_path).await;
        result
    }
}

/// Acquire an advisory file lock at `path`, waiting for a competing holder
/// to release it. If `timeout` elapses with the lock still held, reclaim
/// it rather than hanging forever - a crashed holder must not wedge every
/// future run.
async fn acquire_file_lock(path: &Path, timeout: Duration) -> ExecResult<()> {
    let start = std::time::Instant::now();
    loop {
        match tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .await
        {
            Ok(_) => return Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                if start.elapsed() > timeout {
                    let _ = tokio::fs::remove_file(path).await;
                    continue;
                }
                let delay = Duration::from_millis(200 + rand::random::<u64>() % 400);
                tokio::time::sleep(delay).await;
            }
            Err(e) => return Err(e.into()),
        }
    }
}

async fn is_binary_available(path: &Path) -> bool {
    matches!(tokio::fs::metadata(path).await, Ok(meta) if meta.len() > 0)
}

async fn fetch_latest_version() -> ExecResult<String> {
    let url = "https://api.releases.hashicorp.com/v1/releases/terraform/latest";
    let resp: serde_json::Value = reqwest::get(url)
        .await
        .map_err(|e| ExecError::Version(format!("failed to fetch latest terraform version: {e}")))?
        .json()
        .await
        .map_err(|e| ExecError::Version(format!("failed to parse latest-version response: {e}")))?;
    resp["version"].as_str().map(str::to_string).ok_or_else(|| {
        ExecError::Version("latest-version response missing 'version' field".to_string())
    })
}

fn os_arch_string() -> ExecResult<String> {
    let os = match std::env::consts::OS {
        "linux" => "linux",
        "macos" => "darwin",
        "windows" => "windows",
        other => return Err(ExecError::Version(format!("unsupported OS: {other}"))),
    };
    let arch = match std::env::consts::ARCH {
        "x86" => "386",
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "arm" => "arm",
        other => {
            return Err(ExecError::Version(format!(
                "unsupported architecture: {other}"
            )))
        }
    };
    Ok(format!("{os}_{arch}"))
}

async fn download_terraform(version_dir: &Path, version: &str) -> ExecResult<()> {
    tokio::fs::create_dir_all(version_dir).await?;
    let os_arch = os_arch_string()?;
    let url = format!(
        "https://releases.hashicorp.com/terraform/{version}/terraform_{version}_{os_arch}.zip"
    );

    let bytes = reqwest::get(&url)
        .await
        .and_then(|r| r.error_for_status())
        .map_err(|e| ExecError::Version(format!("failed to download terraform {version}: {e}")))?
        .bytes()
        .await
        .map_err(|e| {
            ExecError::Version(format!("failed to read terraform {version} download: {e}"))
        })?;

    let version_dir = version_dir.to_path_buf();
    tokio::task::spawn_blocking(move || extract_zip(&bytes, &version_dir))
        .await
        .map_err(|e| ExecError::Version(format!("extraction task join error: {e}")))??;
    Ok(())
}

fn extract_zip(bytes: &[u8], dest: &Path) -> ExecResult<()> {
    let reader = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(reader)
        .map_err(|e| ExecError::Version(format!("failed to read terraform zip: {e}")))?;
    archive
        .extract(dest)
        .map_err(|e| ExecError::Version(format!("failed to extract terraform zip: {e}")))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let bin = dest.join("terraform");
        if let Ok(meta) = std::fs::metadata(&bin) {
            let mut perms = meta.permissions();
            perms.set_mode(0o755);
            let _ = std::fs::set_permissions(&bin, perms);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn path_resolver_finds_real_binary_on_path() {
        // `sh` is present in every sandbox this workspace runs tests in.
        let resolver = PathVersionResolver::new("sh");
        let path = resolver.resolve(None).await.unwrap();
        assert_eq!(path, PathBuf::from("sh"));
    }

    #[tokio::test]
    async fn path_resolver_errors_on_missing_binary() {
        let resolver = PathVersionResolver::new("cubtera-definitely-not-a-real-binary");
        assert!(resolver.resolve(None).await.is_err());
    }

    #[test]
    fn os_arch_string_is_well_formed() {
        let s = os_arch_string().unwrap();
        assert!(s.contains('_'));
    }

    #[tokio::test]
    async fn file_lock_reclaims_after_timeout_instead_of_hanging() {
        let tmp = tempfile::TempDir::new().unwrap();
        let lock_path = tmp.path().join(".1.9.0.lock");
        // Simulate a stale lock left behind by a crashed holder.
        tokio::fs::write(&lock_path, b"").await.unwrap();

        let result = tokio::time::timeout(
            Duration::from_secs(5),
            acquire_file_lock(&lock_path, Duration::from_millis(50)),
        )
        .await;
        assert!(
            result.is_ok(),
            "acquire_file_lock must not hang past its timeout"
        );
        assert!(result.unwrap().is_ok());
    }

    #[tokio::test]
    async fn file_lock_is_immediately_acquired_when_absent() {
        let tmp = tempfile::TempDir::new().unwrap();
        let lock_path = tmp.path().join(".1.9.0.lock");
        acquire_file_lock(&lock_path, Duration::from_secs(5))
            .await
            .unwrap();
        assert!(lock_path.exists());
    }
}
