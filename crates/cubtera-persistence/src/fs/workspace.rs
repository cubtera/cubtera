//! Filesystem `Workspace` adapter
//!
//! Executes a [`MaterializationPlan`] built by [`cubtera_domain::Unit::materialize`]
//! against the real filesystem. All the "what to do" decisions were already
//! made in the domain; this adapter only does the "how" - actual I/O,
//! off the async runtime's reactor thread via `spawn_blocking`.

use async_trait::async_trait;
use cubtera_core::error::{AppError, AppResult};
use cubtera_core::ports::Workspace;
use cubtera_domain::{MaterializationPlan, MaterializationStep};
use std::path::{Path, PathBuf};

/// Applies materialization plans to the local filesystem.
#[derive(Debug, Clone, Default)]
pub struct FsWorkspace;

impl FsWorkspace {
    /// Create a new filesystem workspace adapter
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Workspace for FsWorkspace {
    async fn apply(&self, plan: &MaterializationPlan) -> AppResult<()> {
        let temp_folder = plan.temp_folder.clone();
        tokio::fs::create_dir_all(&temp_folder)
            .await
            .map_err(|e| AppError::io(format!("failed to create {:?}: {e}", temp_folder)))?;

        for step in &plan.steps {
            apply_step(step).await?;
        }
        Ok(())
    }

    async fn clean(&self, temp_folder: &Path) -> AppResult<()> {
        if tokio::fs::try_exists(temp_folder).await.unwrap_or(false) {
            tokio::fs::remove_dir_all(temp_folder)
                .await
                .map_err(|e| AppError::io(format!("failed to remove {:?}: {e}", temp_folder)))?;
        }
        Ok(())
    }

    async fn read_file(&self, path: &Path) -> AppResult<Option<String>> {
        match tokio::fs::read_to_string(path).await {
            Ok(content) => Ok(Some(content)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(AppError::io(format!("failed to read {:?}: {e}", path))),
        }
    }
}

async fn apply_step(step: &MaterializationStep) -> AppResult<()> {
    match step {
        MaterializationStep::Symlink { target, link } => symlink(target, link).await,
        MaterializationStep::CopyDir { src, dst } => copy_dir(src, dst).await,
        MaterializationStep::CopyFile { src, dst, required } => {
            copy_file(src, dst, *required).await
        }
        MaterializationStep::WriteFile { path, content } => write_file(path, content).await,
    }
}

async fn symlink(target: &Path, link: &Path) -> AppResult<()> {
    // `try_exists` follows symlinks, resolving a relative target against
    // the *link's* own directory rather than the process CWD used to build
    // `target` - a relative target that's valid from CWD (e.g. modulesPath
    // configured as a repo-relative path) reads as a "dangling" link from
    // that perspective, causing `symlink()` to attempt recreating an
    // already-present link and fail with `EEXIST` on every rematerialize.
    // `symlink_metadata` (lstat) checks the link path itself, not its
    // target, so it's the correct idempotency check here.
    if tokio::fs::symlink_metadata(link).await.is_ok() {
        return Ok(());
    }
    if !tokio::fs::try_exists(target).await.unwrap_or(false) {
        // The modules directory is optional at the unit level - not every
        // unit type needs it, so a missing target is not an error here.
        return Ok(());
    }
    // Canonicalize so the link stores an absolute target: a relative target
    // (e.g. a repo-relative `modulesPath` like "example/modules") is only
    // valid resolved against the *process's* CWD, but the OS resolves a
    // relative symlink target against the *link's own* directory - leaving
    // it relative would produce a link that's dangling from every consumer
    // (`helm template`, `ls -L`, etc.) that isn't run from that exact CWD.
    let target = tokio::fs::canonicalize(target)
        .await
        .map_err(|e| AppError::io(format!("failed to canonicalize {:?}: {e}", target)))?;
    let link = link.to_path_buf();
    tokio::task::spawn_blocking(move || make_symlink(&target, &link))
        .await
        .map_err(|e| AppError::io(format!("symlink task join error: {e}")))??;
    Ok(())
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> AppResult<()> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| AppError::io(format!("failed to symlink {:?} -> {:?}: {e}", link, target)))
}

#[cfg(not(unix))]
fn make_symlink(target: &Path, link: &Path) -> AppResult<()> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|e| AppError::io(format!("failed to symlink {:?} -> {:?}: {e}", link, target)))
}

async fn copy_dir(src: &Path, dst: &Path) -> AppResult<()> {
    if !tokio::fs::try_exists(src).await.unwrap_or(false) {
        return Err(AppError::io(format!(
            "cannot copy dir, source missing: {:?}",
            src
        )));
    }
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_recursive(&src, &dst))
        .await
        .map_err(|e| AppError::io(format!("copy dir task join error: {e}")))??;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> AppResult<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| AppError::io(format!("failed to create dir {:?}: {e}", dst)))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| AppError::io(format!("failed to read dir {:?}: {e}", src)))?
    {
        let entry = entry.map_err(|e| AppError::io(format!("failed to read dir entry: {e}")))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                AppError::io(format!(
                    "failed to copy {:?} -> {:?}: {e}",
                    src_path, dst_path
                ))
            })?;
        }
    }
    Ok(())
}

async fn copy_file(src: &Path, dst: &Path, required: bool) -> AppResult<()> {
    let src = expand_home(src);
    if !tokio::fs::try_exists(&src).await.unwrap_or(false) {
        if required {
            return Err(AppError::io(format!("required file missing: {:?}", src)));
        }
        tracing::warn!(source = ?src, "optional file missing, skipping");
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::io(format!("failed to create dir {:?}: {e}", parent)))?;
    }
    tokio::fs::copy(&src, dst)
        .await
        .map_err(|e| AppError::io(format!("failed to copy {:?} -> {:?}: {e}", src, dst)))?;
    Ok(())
}

async fn write_file(path: &Path, content: &str) -> AppResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| AppError::io(format!("failed to create dir {:?}: {e}", parent)))?;
    }
    tokio::fs::write(path, content)
        .await
        .map_err(|e| AppError::io(format!("failed to write {:?}: {e}", path)))
}

/// Expand a leading `~` to `$HOME`. This is host-environment interpretation
/// of a path string (an infrastructure concern), which is why it lives in
/// the adapter rather than in `Unit::materialize` - the domain only knows
/// the literal source string from `manifest.spec.files`.
fn expand_home(path: &Path) -> PathBuf {
    let Some(s) = path.to_str() else {
        return path.to_path_buf();
    };
    if let Some(rest) = s.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_domain::MaterializationPlan;
    use tempfile::TempDir;

    #[tokio::test]
    async fn apply_writes_file_and_creates_temp_folder() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");

        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::WriteFile {
            path: temp_folder.join("cubtera_ext.json"),
            content: "{}".to_string(),
        });

        FsWorkspace::new().apply(&plan).await.unwrap();

        let content = tokio::fs::read_to_string(temp_folder.join("cubtera_ext.json"))
            .await
            .unwrap();
        assert_eq!(content, "{}");
    }

    #[tokio::test]
    async fn apply_copies_directory_recursively() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        tokio::fs::create_dir_all(src.join("nested")).await.unwrap();
        tokio::fs::write(src.join("a.txt"), "a").await.unwrap();
        tokio::fs::write(src.join("nested/b.txt"), "b")
            .await
            .unwrap();

        let temp_folder = tmp.path().join("unit");
        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::CopyDir {
            src: src.clone(),
            dst: temp_folder.clone(),
        });

        FsWorkspace::new().apply(&plan).await.unwrap();

        assert_eq!(
            tokio::fs::read_to_string(temp_folder.join("a.txt"))
                .await
                .unwrap(),
            "a"
        );
        assert_eq!(
            tokio::fs::read_to_string(temp_folder.join("nested/b.txt"))
                .await
                .unwrap(),
            "b"
        );
    }

    #[tokio::test]
    async fn apply_fails_on_missing_required_copy_dir_source() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::CopyDir {
            src: tmp.path().join("does-not-exist"),
            dst: temp_folder.clone(),
        });

        assert!(FsWorkspace::new().apply(&plan).await.is_err());
    }

    #[tokio::test]
    async fn apply_skips_missing_optional_file() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::CopyFile {
            src: tmp.path().join("does-not-exist.txt"),
            dst: temp_folder.join("optional.txt"),
            required: false,
        });

        FsWorkspace::new().apply(&plan).await.unwrap();
        assert!(!temp_folder.join("optional.txt").exists());
    }

    #[tokio::test]
    async fn clean_removes_existing_temp_folder() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        tokio::fs::create_dir_all(&temp_folder).await.unwrap();
        tokio::fs::write(temp_folder.join("f.txt"), "x")
            .await
            .unwrap();

        FsWorkspace::new().clean(&temp_folder).await.unwrap();
        assert!(!temp_folder.exists());
    }

    #[tokio::test]
    async fn clean_is_noop_when_missing() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("does-not-exist");
        FsWorkspace::new().clean(&temp_folder).await.unwrap();
    }

    #[tokio::test]
    async fn read_file_returns_content_when_present() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cubtera_outputs.json");
        tokio::fs::write(&path, "{}").await.unwrap();

        let content = FsWorkspace::new().read_file(&path).await.unwrap();
        assert_eq!(content, Some("{}".to_string()));
    }

    #[tokio::test]
    async fn read_file_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does-not-exist.json");

        let content = FsWorkspace::new().read_file(&path).await.unwrap();
        assert_eq!(content, None);
    }
}
