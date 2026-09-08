//! Git-backed [`SourceRepo`]: pins by commit.
//!
//! Every read goes through `git show <rev>:<path>` for content and
//! `git ls-tree -r --name-only <rev> -- <subpath>` for the file list - both
//! read from git's object store at an exact revision, not the working
//! tree. A dirty checkout (uncommitted edits, an in-progress `git pull`)
//! never changes what a pinned revision resolves to, which is the entire
//! point of pinning: v2's module symlink has no such guarantee at all (see
//! `cubtera-model::UnitPackage`'s doc comment).

use crate::{ResolvedTree, SourceError, SourceRepo, SourceResult};
use async_trait::async_trait;
use cubtera_kernel::SafeSegment;
use std::path::PathBuf;
use tokio::process::Command;

pub struct GitSource {
    repo_root: PathBuf,
}

impl GitSource {
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    async fn run(&self, args: &[&str]) -> SourceResult<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(args)
            .output()
            .await
            .map_err(|e| SourceError::git(format!("failed to spawn git: {e}")))?;
        if !output.status.success() {
            return Err(SourceError::git(format!(
                "git {} failed: {}",
                args.join(" "),
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        String::from_utf8(output.stdout)
            .map_err(|e| SourceError::git(format!("non-utf8 output: {e}")))
    }

    /// Reject anything that isn't a safe relative pathspec before it ever
    /// becomes a `git` argument - same kernel seam as every other
    /// path-constructing boundary (docs/specs/2026-09-03-cubtera-v3-architecture.md
    /// ยง4). An empty `subpath` means "the whole tree".
    fn validate_subpath(subpath: &str) -> SourceResult<()> {
        if subpath.is_empty() {
            return Ok(());
        }
        SafeSegment::split_relative_path(subpath)
            .map(|_| ())
            .map_err(|e| SourceError::InvalidReference(subpath.to_string(), e.to_string()))
    }

    async fn list_files_at(&self, rev: &str, subpath: &str) -> SourceResult<ResolvedTree> {
        Self::validate_subpath(subpath)?;
        // `--` guards against a subpath that happens to look like a git
        // option (e.g. a leading `-`) being parsed as one.
        let mut args = vec!["ls-tree", "-r", "--name-only", rev, "--"];
        if !subpath.is_empty() {
            args.push(subpath);
        } else {
            args.push(".");
        }
        let listing = self.run(&args).await?;

        let mut files = Vec::new();
        for path in listing.lines().filter(|l| !l.is_empty()) {
            let content = self.show(rev, path).await?;
            // Strip the resolved `subpath` prefix so a module resolved at
            // "modules/network" gets file paths relative to *itself*
            // ("main.tf"), matching `FsSource::resolve_module`'s shape.
            let relative = if subpath.is_empty() {
                path.to_string()
            } else {
                path.strip_prefix(subpath)
                    .unwrap_or(path)
                    .trim_start_matches('/')
                    .to_string()
            };
            files.push((relative, content));
        }
        Ok(ResolvedTree::from_files(files))
    }

    async fn show(&self, rev: &str, path: &str) -> SourceResult<Vec<u8>> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.repo_root)
            .args(["show", &format!("{rev}:{path}")])
            .output()
            .await
            .map_err(|e| SourceError::git(format!("failed to spawn git: {e}")))?;
        if !output.status.success() {
            return Err(SourceError::git(format!(
                "git show {rev}:{path} failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            )));
        }
        Ok(output.stdout)
    }
}

#[async_trait]
impl SourceRepo for GitSource {
    async fn revision(&self) -> SourceResult<String> {
        let sha = self.run(&["rev-parse", "HEAD"]).await?;
        Ok(sha.trim().to_string())
    }

    async fn list_files(&self, subpath: &str) -> SourceResult<ResolvedTree> {
        self.list_files_at("HEAD", subpath).await
    }

    /// `source_ref` syntax: `"<git-ref>"` or `"<git-ref>:<subpath>"` - e.g.
    /// `"main"` (whole tree at `main`) or `"v1.2.0:modules/network"` (just
    /// that subtree, pinned to a tag). The ref is resolved to a concrete
    /// commit SHA first, so the returned [`ResolvedTree`]'s content is
    /// exactly what that ref pointed to at resolution time, independent of
    /// the ref moving later (a branch being fast-forwarded, for instance).
    async fn resolve_module(&self, source_ref: &str) -> SourceResult<ResolvedTree> {
        let (git_ref, subpath) = match source_ref.split_once(':') {
            Some((r, p)) => (r, p),
            None => (source_ref, ""),
        };
        if git_ref.is_empty() {
            return Err(SourceError::InvalidReference(
                source_ref.to_string(),
                "missing git ref".to_string(),
            ));
        }
        let resolved_sha = self
            .run(&["rev-parse", git_ref])
            .await
            .map_err(|e| SourceError::InvalidReference(source_ref.to_string(), e.to_string()))?;
        self.list_files_at(resolved_sha.trim(), subpath).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::tempdir;
    use tokio::process::Command;

    async fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .status()
            .await
            .unwrap();
        assert!(status.success(), "git {args:?} failed");
    }

    async fn init_repo() -> tempfile::TempDir {
        let dir = tempdir().unwrap();
        git(dir.path(), &["init", "-q"]).await;
        git(dir.path(), &["config", "user.email", "test@example.com"]).await;
        git(dir.path(), &["config", "user.name", "Test"]).await;
        dir
    }

    async fn write_and_commit(dir: &Path, rel: &str, content: &str, message: &str) {
        let path = dir.join(rel);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&path, content).await.unwrap();
        git(dir, &["add", "."]).await;
        git(dir, &["commit", "-q", "-m", message]).await;
    }

    #[tokio::test]
    async fn revision_returns_head_sha() {
        let dir = init_repo().await;
        write_and_commit(dir.path(), "a.txt", "a", "initial").await;
        let source = GitSource::new(dir.path());

        let rev = source.revision().await.unwrap();
        assert_eq!(rev.len(), 40, "expected a full 40-char SHA-1, got {rev:?}");
    }

    #[tokio::test]
    async fn list_files_reads_committed_content() {
        let dir = init_repo().await;
        write_and_commit(dir.path(), "a.txt", "hello", "initial").await;
        let source = GitSource::new(dir.path());

        let tree = source.list_files("").await.unwrap();
        assert_eq!(tree.files, vec![("a.txt".to_string(), b"hello".to_vec())]);
    }

    #[tokio::test]
    async fn list_files_ignores_uncommitted_working_tree_changes() {
        // The whole point of pinning by commit: a dirty working tree must
        // not change what HEAD resolves to.
        let dir = init_repo().await;
        write_and_commit(dir.path(), "a.txt", "committed", "initial").await;
        tokio::fs::write(dir.path().join("a.txt"), "dirty-uncommitted")
            .await
            .unwrap();
        let source = GitSource::new(dir.path());

        let tree = source.list_files("").await.unwrap();
        assert_eq!(
            tree.files,
            vec![("a.txt".to_string(), b"committed".to_vec())]
        );
    }

    #[tokio::test]
    async fn resolve_module_pins_to_a_tag_independent_of_later_commits() {
        let dir = init_repo().await;
        write_and_commit(dir.path(), "modules/network/main.tf", "v1", "initial").await;
        git(dir.path(), &["tag", "v1.0.0"]).await;
        write_and_commit(dir.path(), "modules/network/main.tf", "v2", "update").await;
        let source = GitSource::new(dir.path());

        let pinned = source
            .resolve_module("v1.0.0:modules/network")
            .await
            .unwrap();
        assert_eq!(pinned.files, vec![("main.tf".to_string(), b"v1".to_vec())]);

        let latest = source.resolve_module("HEAD:modules/network").await.unwrap();
        assert_eq!(latest.files, vec![("main.tf".to_string(), b"v2".to_vec())]);
    }

    #[tokio::test]
    async fn resolve_module_rejects_unknown_ref() {
        let dir = init_repo().await;
        write_and_commit(dir.path(), "a.txt", "a", "initial").await;
        let source = GitSource::new(dir.path());
        assert!(source.resolve_module("does-not-exist").await.is_err());
    }

    #[tokio::test]
    async fn list_files_rejects_path_traversal() {
        let dir = init_repo().await;
        write_and_commit(dir.path(), "a.txt", "a", "initial").await;
        let source = GitSource::new(dir.path());
        assert!(source.list_files("../../etc").await.is_err());
    }
}
