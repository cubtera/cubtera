//! `SourceRepo` port: where inventory/unit/module content actually comes
//! from, decoupled from how `cubtera-model`/`cubtera-app` consume it.
//!
//! v2 has no such abstraction - `modulesPath` is a plain `Path` symlinked
//! straight into a unit's temp folder (see `cubtera-model`'s
//! `UnitPackage` doc comment for why that's a reproducibility bug, H12 in
//! the prior architecture review), and there is no equivalent notion for
//! "what revision was the inventory at when this ran" at all. `SourceRepo`
//! gives both a real answer: [`GitSource`] pins by commit (reading blob
//! content at that commit via `git show`, not the working tree, so a
//! dirty checkout never silently changes what a pinned revision means),
//! [`FsSource`] is the honest fallback for a non-git inventory/module
//! source - a content-hash snapshot with no history, not a fake revision
//! string.
//!
//! This crate depends on `cubtera-kernel` only (see
//! docs/specs/2026-09-03-cubtera-v3-architecture.md ยง3's crate table) -
//! not `cubtera-model`, so [`ResolvedTree`] is a local, minimal type;
//! `cubtera-app` (which depends on both) is what turns a `ResolvedTree`
//! into a `cubtera_model::PinnedModule`/`UnitPackage`.

mod error;
mod fs_source;
mod git_source;

pub use error::SourceError;
pub use fs_source::FsSource;
pub use git_source::GitSource;

use async_trait::async_trait;
use cubtera_kernel::Digest;

pub type SourceResult<T> = Result<T, SourceError>;

/// The result of listing files under some subpath, or resolving a module
/// reference: every file's relative path and content, plus a single
/// content hash over all of them (order-independent - see
/// `Digest::of_parts`'s doc comment on why parts are length-prefixed and
/// sorted, mirrored here).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTree {
    pub content_hash: Digest,
    pub files: Vec<(String, Vec<u8>)>,
}

impl ResolvedTree {
    /// Build a `ResolvedTree` from unsorted `(path, content)` pairs,
    /// sorting by path and hashing deterministically.
    pub fn from_files(mut files: Vec<(String, Vec<u8>)>) -> Self {
        files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut parts: Vec<Vec<u8>> = Vec::with_capacity(files.len() * 2);
        for (path, content) in &files {
            parts.push(path.clone().into_bytes());
            parts.push(content.clone());
        }
        Self {
            content_hash: Digest::of_parts(parts),
            files,
        }
    }
}

/// Where inventory/unit/module content is read from.
#[async_trait]
pub trait SourceRepo: Send + Sync {
    /// A stable identifier for the *entire* source's current state - a git
    /// commit SHA for [`GitSource`], a content-hash snapshot id (not a
    /// real history marker) for [`FsSource`]. Part of
    /// `ResolutionManifest` (docs/specs/2026-09-03-cubtera-v3-architecture.md
    /// ยง5.3) - "what inventory revision was actually used" is unanswerable
    /// in v2 today.
    async fn revision(&self) -> SourceResult<String>;

    /// Every file under `subpath` (relative to this source's root),
    /// recursively, with its content - the input to
    /// `cubtera_model::UnitPackage::compute`'s `files` parameter.
    async fn list_files(&self, subpath: &str) -> SourceResult<ResolvedTree>;

    /// Resolve a module reference (`source_ref`'s syntax is
    /// implementation-defined - a git ref for [`GitSource`], a relative
    /// path for [`FsSource`]) to its pinned content - never "whatever is
    /// at this path right now" once resolved, since the returned
    /// [`ResolvedTree`] is an immutable snapshot, not a live handle.
    async fn resolve_module(&self, source_ref: &str) -> SourceResult<ResolvedTree>;
}
