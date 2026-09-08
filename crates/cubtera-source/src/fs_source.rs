//! Plain-filesystem [`SourceRepo`]: a content-hash snapshot with no
//! history. The honest fallback for an inventory/module tree that isn't a
//! git repo - `revision()` never claims to be a commit SHA, it's always
//! exactly what it is: a hash of the current tree's content.

use crate::{ResolvedTree, SourceError, SourceRepo, SourceResult};
use async_trait::async_trait;
use cubtera_kernel::SafeSegment;
use std::path::{Path, PathBuf};

pub struct FsSource {
    root: PathBuf,
}

impl FsSource {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Resolve `subpath` against `self.root`, rejecting traversal - the
    /// same kernel seam every other path-constructing boundary in this
    /// workspace uses (docs/specs/2026-09-03-cubtera-v3-architecture.md
    /// ยง4). An empty `subpath` means "the source root itself".
    fn resolve(&self, subpath: &str) -> SourceResult<PathBuf> {
        if subpath.is_empty() {
            return Ok(self.root.clone());
        }
        let segments = SafeSegment::split_relative_path(subpath)
            .map_err(|e| SourceError::InvalidReference(subpath.to_string(), e.to_string()))?;
        let mut path = self.root.clone();
        for segment in segments {
            path.push(segment.as_str());
        }
        Ok(path)
    }

    async fn read_tree(root: &Path, base: &Path) -> SourceResult<Vec<(String, Vec<u8>)>> {
        let mut files = Vec::new();
        read_tree_into(root, base, &mut files).await?;
        Ok(files)
    }
}

/// Boxed recursion helper - `async fn` can't call itself directly.
fn read_tree_into<'a>(
    root: &'a Path,
    dir: &'a Path,
    out: &'a mut Vec<(String, Vec<u8>)>,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = SourceResult<()>> + Send + 'a>> {
    Box::pin(async move {
        let mut entries = tokio::fs::read_dir(dir)
            .await
            .map_err(|e| SourceError::io(format!("{dir:?}: {e}")))?;
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(|e| SourceError::io(e.to_string()))?
        {
            let path = entry.path();
            let file_type = entry
                .file_type()
                .await
                .map_err(|e| SourceError::io(e.to_string()))?;
            if file_type.is_dir() {
                read_tree_into(root, &path, out).await?;
            } else if file_type.is_file() {
                let content = tokio::fs::read(&path)
                    .await
                    .map_err(|e| SourceError::io(format!("{path:?}: {e}")))?;
                let relative = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push((relative, content));
            }
        }
        Ok(())
    })
}

#[async_trait]
impl SourceRepo for FsSource {
    async fn revision(&self) -> SourceResult<String> {
        let files = FsSource::read_tree(&self.root, &self.root).await?;
        let tree = ResolvedTree::from_files(files);
        Ok(format!("fs-snapshot:{}", tree.content_hash))
    }

    async fn list_files(&self, subpath: &str) -> SourceResult<ResolvedTree> {
        let dir = self.resolve(subpath)?;
        if !tokio::fs::try_exists(&dir).await.unwrap_or(false) {
            return Err(SourceError::NotFound(subpath.to_string()));
        }
        let files = FsSource::read_tree(&dir, &dir).await?;
        Ok(ResolvedTree::from_files(files))
    }

    async fn resolve_module(&self, source_ref: &str) -> SourceResult<ResolvedTree> {
        // FsSource has no ref syntax beyond "a relative path under root" -
        // there is no history to pin against, so this is exactly
        // `list_files`.
        self.list_files(source_ref).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn write(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        tokio::fs::create_dir_all(path.parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(path, content).await.unwrap();
    }

    #[tokio::test]
    async fn list_files_reads_nested_tree() {
        let dir = tempdir().unwrap();
        write(dir.path(), "a.txt", "a").await;
        write(dir.path(), "nested/b.txt", "b").await;
        let source = FsSource::new(dir.path());

        let tree = source.list_files("").await.unwrap();
        let mut paths: Vec<_> = tree.files.iter().map(|(p, _)| p.clone()).collect();
        paths.sort();
        assert_eq!(paths, vec!["a.txt", "nested/b.txt"]);
    }

    #[tokio::test]
    async fn list_files_is_deterministic_regardless_of_creation_order() {
        let dir = tempdir().unwrap();
        write(dir.path(), "z.txt", "z").await;
        write(dir.path(), "a.txt", "a").await;
        let source = FsSource::new(dir.path());
        let tree1 = source.list_files("").await.unwrap();
        let tree2 = source.list_files("").await.unwrap();
        assert_eq!(tree1.content_hash, tree2.content_hash);
    }

    #[tokio::test]
    async fn list_files_rejects_path_traversal() {
        let dir = tempdir().unwrap();
        let source = FsSource::new(dir.path());
        assert!(source.list_files("../../etc").await.is_err());
    }

    #[tokio::test]
    async fn list_files_errors_on_missing_subpath() {
        let dir = tempdir().unwrap();
        let source = FsSource::new(dir.path());
        assert!(source.list_files("does-not-exist").await.is_err());
    }

    #[tokio::test]
    async fn revision_changes_when_content_changes() {
        let dir = tempdir().unwrap();
        write(dir.path(), "a.txt", "a").await;
        let source = FsSource::new(dir.path());
        let rev1 = source.revision().await.unwrap();

        write(dir.path(), "a.txt", "a-modified").await;
        let rev2 = source.revision().await.unwrap();

        assert_ne!(rev1, rev2);
        assert!(rev1.starts_with("fs-snapshot:"));
    }

    #[tokio::test]
    async fn resolve_module_reads_subdirectory() {
        let dir = tempdir().unwrap();
        write(dir.path(), "modules/network/main.tf", "resource {}").await;
        let source = FsSource::new(dir.path());

        let resolved = source.resolve_module("modules/network").await.unwrap();
        assert_eq!(resolved.files.len(), 1);
        assert_eq!(resolved.files[0].0, "main.tf");
    }
}
