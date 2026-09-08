//! Content-addressed unit packages.
//!
//! v2 resolves a unit's modules by symlinking `tempFolder/modules` at
//! `modulesPath`, a single shared, mutable directory - every unit instance
//! that ever ran points at the *same* live directory, so an in-place
//! `terraform get -update`/manual edit changes what every already-applied
//! instance would materialize next time, with no record of what version it
//! last actually ran against (flagged as H12 in the prior architecture
//! review, "modules-symlink-write-through").
//!
//! [`UnitPackage`] is the fix: a single content hash over the unit's own
//! manifest bytes, its own file tree, and every module it depends on -
//! each module pinned by its own content hash, not "whatever happens to be
//! at this path right now". Two units with the same package content hash
//! are guaranteed to run against byte-identical inputs; a module symlink
//! being edited out from under a running unit becomes a hash mismatch you
//! can detect, not a silent behavior change.

use cubtera_kernel::{Digest, Ident};

/// One pinned module dependency: a name, a human-readable source
/// reference (a git ref, an OCI digest string, a path - `cubtera-source`
/// decides how to interpret it), and the content hash of what it actually
/// resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinnedModule {
    pub name: Ident,
    pub source_ref: String,
    pub content_hash: Digest,
}

impl PinnedModule {
    pub fn new(name: Ident, source_ref: impl Into<String>, content_hash: Digest) -> Self {
        Self {
            name,
            source_ref: source_ref.into(),
            content_hash,
        }
    }
}

/// A unit's full content identity: its manifest, its own files, and every
/// pinned module, boiled down to one [`Digest`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnitPackage {
    pub manifest_digest: Digest,
    pub files_hash: Digest,
    pub pinned_modules: Vec<PinnedModule>,
    pub content_hash: Digest,
}

impl UnitPackage {
    /// Compute a `UnitPackage` from raw inputs.
    ///
    /// - `manifest_bytes`: the raw `unit.toml` (or, during migration,
    ///   `manifest.toml`) bytes, hashed as-is - two manifests that differ
    ///   only in formatting/comments are, deliberately, different packages
    ///   (this is a reproducibility primitive, not a semantic diff).
    /// - `files`: every other file under the unit's own directory, as
    ///   `(relative_path, content)` pairs, in *any* order - this function
    ///   sorts by path itself, so the hash never depends on directory
    ///   listing order (the same "never trust iteration order" rule
    ///   `InstanceId::digest` applies to `dims`/`ext`).
    /// - `pinned_modules`: already-resolved module pins, in any order (also
    ///   sorted internally, by name).
    pub fn compute(
        manifest_bytes: &[u8],
        files: &[(String, Vec<u8>)],
        mut pinned_modules: Vec<PinnedModule>,
    ) -> Self {
        let manifest_digest = Digest::of(manifest_bytes);

        let mut sorted_files: Vec<&(String, Vec<u8>)> = files.iter().collect();
        sorted_files.sort_by(|a, b| a.0.cmp(&b.0));
        let mut file_parts: Vec<Vec<u8>> = Vec::with_capacity(sorted_files.len() * 2);
        for (path, content) in &sorted_files {
            file_parts.push(path.clone().into_bytes());
            file_parts.push(content.clone());
        }
        let files_hash = Digest::of_parts(file_parts);

        pinned_modules.sort_by(|a, b| a.name.cmp(&b.name));
        let mut content_parts: Vec<Vec<u8>> = vec![
            manifest_digest.to_hex().into_bytes(),
            files_hash.to_hex().into_bytes(),
        ];
        for module in &pinned_modules {
            content_parts.push(module.name.to_string().into_bytes());
            content_parts.push(module.content_hash.to_hex().into_bytes());
        }
        let content_hash = Digest::of_parts(content_parts);

        Self {
            manifest_digest,
            files_hash,
            pinned_modules,
            content_hash,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn module(name: &str, content: &[u8]) -> PinnedModule {
        PinnedModule::new(
            Ident::parse(name).unwrap(),
            format!("git:{name}@main"),
            Digest::of(content),
        )
    }

    #[test]
    fn deterministic_regardless_of_file_order() {
        let files_a = vec![
            ("a.tf".to_string(), b"a".to_vec()),
            ("b.tf".to_string(), b"b".to_vec()),
        ];
        let files_b = vec![
            ("b.tf".to_string(), b"b".to_vec()),
            ("a.tf".to_string(), b"a".to_vec()),
        ];
        let pkg_a = UnitPackage::compute(b"manifest", &files_a, vec![]);
        let pkg_b = UnitPackage::compute(b"manifest", &files_b, vec![]);
        assert_eq!(pkg_a.content_hash, pkg_b.content_hash);
    }

    #[test]
    fn deterministic_regardless_of_module_order() {
        let modules_a = vec![module("network", b"1"), module("iam", b"2")];
        let modules_b = vec![module("iam", b"2"), module("network", b"1")];
        let pkg_a = UnitPackage::compute(b"manifest", &[], modules_a);
        let pkg_b = UnitPackage::compute(b"manifest", &[], modules_b);
        assert_eq!(pkg_a.content_hash, pkg_b.content_hash);
    }

    #[test]
    fn differs_when_a_file_changes() {
        let files_a = vec![("a.tf".to_string(), b"a".to_vec())];
        let files_b = vec![("a.tf".to_string(), b"a-modified".to_vec())];
        let pkg_a = UnitPackage::compute(b"manifest", &files_a, vec![]);
        let pkg_b = UnitPackage::compute(b"manifest", &files_b, vec![]);
        assert_ne!(pkg_a.content_hash, pkg_b.content_hash);
        assert_eq!(pkg_a.manifest_digest, pkg_b.manifest_digest);
    }

    #[test]
    fn differs_when_a_module_pin_changes() {
        let pkg_a = UnitPackage::compute(b"manifest", &[], vec![module("network", b"v1")]);
        let pkg_b = UnitPackage::compute(b"manifest", &[], vec![module("network", b"v2")]);
        assert_ne!(pkg_a.content_hash, pkg_b.content_hash);
    }

    #[test]
    fn differs_when_manifest_changes() {
        let pkg_a = UnitPackage::compute(b"manifest-a", &[], vec![]);
        let pkg_b = UnitPackage::compute(b"manifest-b", &[], vec![]);
        assert_ne!(pkg_a.content_hash, pkg_b.content_hash);
    }
}
