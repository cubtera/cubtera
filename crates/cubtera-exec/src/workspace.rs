//! Rooted workspace handle.
//!
//! `Workspace` replaces v2's pattern of "join a raw string onto a
//! `PathBuf`, then hope a validation check ran somewhere upstream" (the
//! shape of the path-escape bug the P0 kernel seam closed at the existing
//! v2 boundaries). Here containment is a type-level property: the only way
//! to obtain a [`RootedPath`] is [`Workspace::join`] /
//! [`Workspace::join_relative`], both of which only accept already-rejected-
//! traversal input (`SafeSegment`, or a `/`-separated string split through
//! `SafeSegment::split_relative_path`). There is no public constructor for
//! `RootedPath` that skips validation - "validate then use a different
//! unchecked path" is not expressible.

use crate::error::{ExecError, ExecResult};
use cubtera_kernel::SafeSegment;
use std::path::{Path, PathBuf};

/// A path that has been proven to live under some [`Workspace`]'s root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootedPath(PathBuf);

impl RootedPath {
    pub fn as_path(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Path> for RootedPath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

/// A rooted handle onto a directory. Every operation is confined to
/// `root` by construction: `join`/`join_relative` reject anything that
/// would climb out (`..`, absolute paths, path separators smuggled inside a
/// single segment), so there is no separate "containment check" step to
/// forget.
#[derive(Debug, Clone)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Create a handle rooted at `root`. `root` need not exist yet - a
    /// workspace is often a not-yet-created temp folder; call
    /// [`Workspace::ensure_dir`] before writing into it.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Join a sequence of already-validated segments onto the root.
    pub fn join(&self, segments: &[SafeSegment]) -> RootedPath {
        let mut full = self.root.clone();
        for seg in segments {
            full.push(seg.as_str());
        }
        RootedPath(full)
    }

    /// Join a `/`-separated relative path string, validating every
    /// component first. Rejects `..`, absolute paths, and empty input -
    /// the exact shapes that let v2's `spec.files` destinations escape
    /// `tempFolderPath` before the P0 kernel seam.
    pub fn join_relative(&self, relative: &str) -> ExecResult<RootedPath> {
        let segments = SafeSegment::split_relative_path(relative)
            .map_err(|e| ExecError::Containment(format!("{relative:?}: {e}")))?;
        Ok(self.join(&segments))
    }

    /// Create the root directory (and any missing parents) if it doesn't
    /// exist yet.
    pub async fn ensure_dir(&self) -> ExecResult<()> {
        tokio::fs::create_dir_all(&self.root).await?;
        Ok(())
    }

    /// Write `contents` to `path`, creating any missing parent directories
    /// first (`path` is always under `root` - it can only have been
    /// constructed by this same `Workspace`).
    pub async fn write_file(
        &self,
        path: &RootedPath,
        contents: impl AsRef<[u8]>,
    ) -> ExecResult<()> {
        if let Some(parent) = path.as_path().parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(path.as_path(), contents.as_ref()).await?;
        Ok(())
    }

    /// Read `path` back as raw bytes.
    pub async fn read_file(&self, path: &RootedPath) -> ExecResult<Vec<u8>> {
        Ok(tokio::fs::read(path.as_path()).await?)
    }

    /// Read `path` back as UTF-8 text.
    pub async fn read_to_string(&self, path: &RootedPath) -> ExecResult<String> {
        Ok(tokio::fs::read_to_string(path.as_path()).await?)
    }

    /// Whether a rooted path currently exists on disk.
    pub async fn exists(&self, path: &RootedPath) -> bool {
        tokio::fs::try_exists(path.as_path()).await.unwrap_or(false)
    }

    /// List the immediate entries of `sub` (relative to `root`; pass `&[]`
    /// for the root itself), returning their file names.
    pub async fn list_dir(&self, sub: &[SafeSegment]) -> ExecResult<Vec<String>> {
        let dir = self.join(sub);
        let mut entries = tokio::fs::read_dir(dir.as_path()).await?;
        let mut names = Vec::new();
        while let Some(entry) = entries.next_entry().await? {
            if let Some(name) = entry.file_name().to_str() {
                names.push(name.to_string());
            }
        }
        Ok(names)
    }

    /// Remove the entire workspace root, ignoring "not found".
    pub async fn remove_all(&self) -> ExecResult<()> {
        match tokio::fs::remove_dir_all(&self.root).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cubtera_kernel::Ident;

    fn seg(raw: &str) -> SafeSegment {
        SafeSegment::parse(raw).unwrap()
    }

    #[test]
    fn join_stays_under_root() {
        let ws = Workspace::new("/tmp/cubtera/unit");
        let path = ws.join(&[seg("main.tf")]);
        assert_eq!(path.as_path(), Path::new("/tmp/cubtera/unit/main.tf"));
    }

    #[test]
    fn join_relative_rejects_traversal() {
        let ws = Workspace::new("/tmp/cubtera/unit");
        for bad in ["../../etc/passwd", "/etc/passwd", "sub/../../etc"] {
            assert!(
                ws.join_relative(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }

    #[test]
    fn join_relative_accepts_nested_relative() {
        let ws = Workspace::new("/tmp/cubtera/unit");
        let path = ws.join_relative("modules/network/main.tf").unwrap();
        assert_eq!(
            path.as_path(),
            Path::new("/tmp/cubtera/unit/modules/network/main.tf")
        );
    }

    #[tokio::test]
    async fn write_then_read_round_trips() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ws = Workspace::new(tmp.path());
        ws.ensure_dir().await.unwrap();

        let path = ws.join(&[seg("cubtera_dim_env.json")]);
        ws.write_file(&path, br#"{"name":"prod"}"#).await.unwrap();

        assert!(ws.exists(&path).await);
        let content = ws.read_to_string(&path).await.unwrap();
        assert_eq!(content, r#"{"name":"prod"}"#);
    }

    #[tokio::test]
    async fn write_file_creates_missing_parent_dirs() {
        let tmp = tempfile::TempDir::new().unwrap();
        let ws = Workspace::new(tmp.path());
        let path = ws.join_relative("sub/dir/out.txt").unwrap();

        ws.write_file(&path, b"ok").await.unwrap();
        assert!(ws.exists(&path).await);
    }

    /// The [`Ident`] grammar (no `/`, no `..`, no NUL - see
    /// `cubtera-kernel`'s property tests) means an org/unit/dim name can
    /// never itself be a traversal payload. Reconfirmed here at the
    /// `Workspace` boundary as an adversarial regression guard, not just in
    /// `cubtera-kernel`'s own property suite.
    #[test]
    fn ident_grammar_rejects_traversal_payloads() {
        for bad in ["..", "../etc", "a/b", "a\\b", ""] {
            assert!(
                Ident::parse(bad).is_err(),
                "expected {bad:?} to be rejected"
            );
        }
    }
}
