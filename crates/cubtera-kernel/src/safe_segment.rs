use crate::error::{IdentRejection, KernelError};
use std::fmt;

/// Maximum length of a single path segment - conservative enough to stay
/// well under every common filesystem's per-component limit (255 bytes on
/// most Unix filesystems, 255 UTF-16 code units on Windows/NTFS).
pub const MAX_SEGMENT_LEN: usize = 255;

/// A single filesystem path *component* - one directory or file name, not a
/// full path. Unlike [`crate::Ident`] this preserves case and allows `.`
/// inside the name (`main.tf`, `.gitignore`), because it models arbitrary
/// include/spec-file names rather than org/unit/dimension identity. What it
/// never allows is anything that could turn a `Workspace::join` call into a
/// traversal: no `/`, no `\`, no NUL, and never exactly `.` or `..`.
///
/// `RootedPath`/`Workspace` (in `cubtera-exec`) only ever accept
/// `&[SafeSegment]`, so "this destination escapes the workspace root" is
/// impossible to construct, not just checked for.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SafeSegment(String);

impl SafeSegment {
    pub fn parse(raw: &str) -> Result<Self, KernelError> {
        if raw.is_empty() {
            return Err(reject(raw, IdentRejection::Empty));
        }
        if raw.contains('\0') {
            return Err(reject(raw, IdentRejection::ContainsNul));
        }
        if raw.contains('/') || raw.contains('\\') {
            return Err(reject(raw, IdentRejection::ContainsPathSeparator));
        }
        if raw == "." || raw == ".." {
            return Err(reject(raw, IdentRejection::ContainsParentRef));
        }
        if raw.len() > MAX_SEGMENT_LEN {
            return Err(reject(raw, IdentRejection::TooLong));
        }
        Ok(Self(raw.to_string()))
    }

    /// Split a `/`-separated relative path into validated segments.
    /// Rejects absolute paths, empty paths, and any `.`/`..` component -
    /// the exact shape of input that let v2's `spec.files` destinations and
    /// `-e`/`-d` extension values escape `tempFolderPath`/`unitStatePath`.
    pub fn split_relative_path(raw: &str) -> Result<Vec<Self>, KernelError> {
        if raw.starts_with('/') || raw.starts_with('\\') {
            return Err(KernelError::InvalidSegment {
                raw: raw.to_string(),
                reason: IdentRejection::ContainsPathSeparator,
            });
        }
        if raw.is_empty() {
            return Err(KernelError::InvalidSegment {
                raw: raw.to_string(),
                reason: IdentRejection::Empty,
            });
        }
        raw.split(['/', '\\']).map(Self::parse).collect()
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn reject(raw: &str, reason: IdentRejection) -> KernelError {
    KernelError::InvalidSegment {
        raw: raw.to_string(),
        reason,
    }
}

impl AsRef<str> for SafeSegment {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for SafeSegment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_filenames() {
        for raw in ["main.tf", ".gitignore", "cubtera_dim_env.json", "a b"] {
            assert!(
                SafeSegment::parse(raw).is_ok(),
                "expected {raw:?} to be valid"
            );
        }
    }

    #[test]
    fn rejects_traversal_and_separators() {
        for raw in ["..", ".", "a/b", "a\\b", "/etc"] {
            assert!(
                SafeSegment::parse(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    #[test]
    fn split_relative_path_rejects_traversal_anywhere() {
        for raw in ["../etc/passwd", "a/../b", "a/..", "sub/../../etc"] {
            assert!(
                SafeSegment::split_relative_path(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    #[test]
    fn split_relative_path_rejects_absolute() {
        assert!(SafeSegment::split_relative_path("/etc/passwd").is_err());
    }

    #[test]
    fn split_relative_path_accepts_nested_relative() {
        let segments = SafeSegment::split_relative_path("sub/dir/file.txt").unwrap();
        assert_eq!(
            segments.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
            vec!["sub", "dir", "file.txt"]
        );
    }
}
