//! Applies a [`cubtera_model::MaterializationPlan`] to the local
//! filesystem - a from-scratch, v3-native port of v2's
//! `cubtera_persistence::fs::FsWorkspace`, with no dependency on
//! `cubtera-core`/`cubtera-persistence`.
//!
//! Every step was already fully decided by `cubtera_model::Unit::materialize`
//! (pure, no I/O) - this module only does the "how": actual filesystem
//! calls, off the async runtime's reactor thread via `spawn_blocking` for
//! anything recursive/blocking.

use crate::error::{ExecError, ExecResult};
use cubtera_model::{MaterializationPlan, MaterializationStep};
use std::path::{Path, PathBuf};

/// Apply every step of `plan` in order (creating `plan.temp_folder` first).
/// Idempotent by construction, matching every individual step's own
/// idempotency (symlinks are skipped if already present, writes/copies
/// overwrite) - see [`MaterializationStep`]'s doc comment.
pub async fn apply(plan: &MaterializationPlan) -> ExecResult<()> {
    tokio::fs::create_dir_all(&plan.temp_folder)
        .await
        .map_err(|e| ExecError::Io(format!("failed to create {:?}: {e}", plan.temp_folder)))?;

    for step in &plan.steps {
        apply_step(step).await?;
    }
    Ok(())
}

/// Remove `temp_folder` entirely, if it exists. No-op if it doesn't.
pub async fn clean(temp_folder: &Path) -> ExecResult<()> {
    if tokio::fs::try_exists(temp_folder).await.unwrap_or(false) {
        tokio::fs::remove_dir_all(temp_folder)
            .await
            .map_err(|e| ExecError::Io(format!("failed to remove {:?}: {e}", temp_folder)))?;
    }
    Ok(())
}

/// Read a file's content, `None` if it doesn't exist - e.g.
/// `cubtera_outputs.json` after a runner's `collect_outputs` step.
pub async fn read_file(path: &Path) -> ExecResult<Option<String>> {
    match tokio::fs::read_to_string(path).await {
        Ok(content) => Ok(Some(content)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ExecError::Io(format!("failed to read {path:?}: {e}"))),
    }
}

async fn apply_step(step: &MaterializationStep) -> ExecResult<()> {
    match step {
        MaterializationStep::Symlink { target, link } => symlink(target, link).await,
        MaterializationStep::CopyDir { src, dst } => copy_dir(src, dst).await,
        MaterializationStep::CopyFile { src, dst, required } => {
            copy_file(src, dst, *required).await
        }
        MaterializationStep::WriteFile { path, content } => write_file(path, content).await,
    }
}

async fn symlink(target: &Path, link: &Path) -> ExecResult<()> {
    // `symlink_metadata` (lstat) checks the link path itself, not its
    // target - the correct idempotency check for "does a link already
    // exist here", independent of whether its target still resolves.
    if tokio::fs::symlink_metadata(link).await.is_ok() {
        return Ok(());
    }
    if !tokio::fs::try_exists(target).await.unwrap_or(false) {
        // The modules directory is optional at the unit level - not every
        // unit type needs it, so a missing target is not an error here.
        return Ok(());
    }
    // Canonicalize so the link stores an absolute target: the OS resolves
    // a relative symlink target against the *link's own* directory, not
    // the process's CWD, so a repo-relative `modulesPath` would otherwise
    // produce a link that's dangling from any other CWD.
    let target = tokio::fs::canonicalize(target)
        .await
        .map_err(|e| ExecError::Io(format!("failed to canonicalize {target:?}: {e}")))?;
    let link = link.to_path_buf();
    tokio::task::spawn_blocking(move || make_symlink(&target, &link))
        .await
        .map_err(|e| ExecError::Io(format!("symlink task join error: {e}")))??;
    Ok(())
}

#[cfg(unix)]
fn make_symlink(target: &Path, link: &Path) -> ExecResult<()> {
    std::os::unix::fs::symlink(target, link)
        .map_err(|e| ExecError::Io(format!("failed to symlink {link:?} -> {target:?}: {e}")))
}

#[cfg(not(unix))]
fn make_symlink(target: &Path, link: &Path) -> ExecResult<()> {
    std::os::windows::fs::symlink_dir(target, link)
        .map_err(|e| ExecError::Io(format!("failed to symlink {link:?} -> {target:?}: {e}")))
}

async fn copy_dir(src: &Path, dst: &Path) -> ExecResult<()> {
    if !tokio::fs::try_exists(src).await.unwrap_or(false) {
        return Err(ExecError::Io(format!(
            "cannot copy dir, source missing: {src:?}"
        )));
    }
    let src = src.to_path_buf();
    let dst = dst.to_path_buf();
    tokio::task::spawn_blocking(move || copy_dir_recursive(&src, &dst))
        .await
        .map_err(|e| ExecError::Io(format!("copy dir task join error: {e}")))??;
    Ok(())
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> ExecResult<()> {
    std::fs::create_dir_all(dst)
        .map_err(|e| ExecError::Io(format!("failed to create dir {dst:?}: {e}")))?;

    for entry in std::fs::read_dir(src)
        .map_err(|e| ExecError::Io(format!("failed to read dir {src:?}: {e}")))?
    {
        let entry = entry.map_err(|e| ExecError::Io(format!("failed to read dir entry: {e}")))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|e| {
                ExecError::Io(format!("failed to copy {src_path:?} -> {dst_path:?}: {e}"))
            })?;
        }
    }
    Ok(())
}

async fn copy_file(src: &Path, dst: &Path, required: bool) -> ExecResult<()> {
    let src = expand_home(src);
    if !tokio::fs::try_exists(&src).await.unwrap_or(false) {
        if required {
            return Err(ExecError::Io(format!("required file missing: {src:?}")));
        }
        tracing::warn!(source = ?src, "optional file missing, skipping");
        return Ok(());
    }
    if let Some(parent) = dst.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ExecError::Io(format!("failed to create dir {parent:?}: {e}")))?;
    }
    tokio::fs::copy(&src, dst)
        .await
        .map_err(|e| ExecError::Io(format!("failed to copy {src:?} -> {dst:?}: {e}")))?;
    Ok(())
}

async fn write_file(path: &Path, content: &str) -> ExecResult<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| ExecError::Io(format!("failed to create dir {parent:?}: {e}")))?;
    }
    tokio::fs::write(path, content)
        .await
        .map_err(|e| ExecError::Io(format!("failed to write {path:?}: {e}")))
}

/// Expand a leading `~` to `$HOME`. Host-environment interpretation of a
/// path string (an infrastructure concern), which is why it lives here
/// rather than in `Unit::materialize` - the model only knows the literal
/// source string from `manifest.spec.files`.
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

        apply(&plan).await.unwrap();

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

        apply(&plan).await.unwrap();

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

        assert!(apply(&plan).await.is_err());
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

        apply(&plan).await.unwrap();
        assert!(!temp_folder.join("optional.txt").exists());
    }

    #[tokio::test]
    async fn apply_fails_on_missing_required_file() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::CopyFile {
            src: tmp.path().join("does-not-exist.txt"),
            dst: temp_folder.join("required.txt"),
            required: true,
        });

        assert!(apply(&plan).await.is_err());
    }

    #[tokio::test]
    async fn later_copy_to_the_same_destination_wins() {
        // Proves the "defaults, then own includes" ordering `Unit::materialize`
        // relies on actually produces "own wins" on disk.
        let tmp = TempDir::new().unwrap();
        let default_src = tmp.path().join("default.txt");
        let own_src = tmp.path().join("own.txt");
        tokio::fs::write(&default_src, "default").await.unwrap();
        tokio::fs::write(&own_src, "own").await.unwrap();

        let temp_folder = tmp.path().join("unit");
        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::CopyFile {
            src: default_src,
            dst: temp_folder.join("notice.txt"),
            required: true,
        });
        plan.push(MaterializationStep::CopyFile {
            src: own_src,
            dst: temp_folder.join("notice.txt"),
            required: true,
        });

        apply(&plan).await.unwrap();

        assert_eq!(
            tokio::fs::read_to_string(temp_folder.join("notice.txt"))
                .await
                .unwrap(),
            "own"
        );
    }

    #[tokio::test]
    async fn symlink_is_idempotent_and_skips_missing_target() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        tokio::fs::create_dir_all(&temp_folder).await.unwrap();

        let mut plan = MaterializationPlan::new(&temp_folder);
        plan.push(MaterializationStep::Symlink {
            target: tmp.path().join("does-not-exist-modules"),
            link: temp_folder.join("modules"),
        });
        apply(&plan).await.unwrap();
        assert!(!temp_folder.join("modules").exists());

        let modules = tmp.path().join("modules");
        tokio::fs::create_dir_all(&modules).await.unwrap();
        let mut plan2 = MaterializationPlan::new(&temp_folder);
        plan2.push(MaterializationStep::Symlink {
            target: modules.clone(),
            link: temp_folder.join("modules"),
        });
        apply(&plan2).await.unwrap();
        assert!(temp_folder.join("modules").exists());

        // Re-applying is a no-op, not an error (EEXIST).
        apply(&plan2).await.unwrap();
    }

    #[tokio::test]
    async fn clean_removes_existing_temp_folder() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("unit");
        tokio::fs::create_dir_all(&temp_folder).await.unwrap();
        tokio::fs::write(temp_folder.join("f.txt"), "x")
            .await
            .unwrap();

        clean(&temp_folder).await.unwrap();
        assert!(!temp_folder.exists());
    }

    #[tokio::test]
    async fn clean_is_noop_when_missing() {
        let tmp = TempDir::new().unwrap();
        let temp_folder = tmp.path().join("does-not-exist");
        clean(&temp_folder).await.unwrap();
    }

    #[tokio::test]
    async fn read_file_returns_content_when_present() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("cubtera_outputs.json");
        tokio::fs::write(&path, "{}").await.unwrap();

        let content = read_file(&path).await.unwrap();
        assert_eq!(content, Some("{}".to_string()));
    }

    #[tokio::test]
    async fn read_file_returns_none_when_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("does-not-exist.json");

        let content = read_file(&path).await.unwrap();
        assert_eq!(content, None);
    }
}
