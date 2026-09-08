use crate::error::{IdentRejection, KernelError};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Maximum length of a single identifier, after lowercasing. Generous
/// enough for real org/unit/dimension names, tight enough to keep every
/// derived filesystem path well under common `PATH_MAX` limits even when
/// several identifiers are joined.
pub const MAX_IDENT_LEN: usize = 128;

/// A single, validated, lowercase-normalized identifier: an org name, a
/// unit name, a dimension type, a dimension name, an extension type/name,
/// or an `[inputs.<alias>]` alias.
///
/// The only way to obtain one is [`Ident::parse`]. Once constructed, an
/// `Ident` is guaranteed to be safe to use as a single filesystem path
/// component, a single SQL identifier value, and a single JSON object key -
/// on every platform this workspace targets - without any further checks at
/// the call site. This is what closes the v2 path-escape bug class: there
/// is no code path that can turn a validated `Ident` back into `..` or an
/// absolute path, because the grammar rejects both at construction, not at
/// the moment of use.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Ident(String);

impl Ident {
    /// Parse and validate a raw string into an `Ident`.
    ///
    /// Grammar (after lowercasing): `^[a-z0-9][a-z0-9_-]*$`, length 1..=128.
    /// Rejects: empty, NUL, `/`, `\`, exactly `.` or `..`, a leading `.` or
    /// `#` (reserved for inventory's own `.default`/`.schema`/hidden-file
    /// conventions), and `:` (reserved for `type:name` references, see
    /// [`crate::DimRef`]).
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
        if raw.starts_with('.') {
            return Err(reject(raw, IdentRejection::LeadingDot));
        }
        if raw.starts_with('#') {
            return Err(reject(raw, IdentRejection::LeadingHash));
        }
        if raw.contains(':') {
            return Err(reject(raw, IdentRejection::ContainsColon));
        }
        if raw.len() > MAX_IDENT_LEN {
            return Err(reject(raw, IdentRejection::TooLong));
        }

        let lower = raw.to_ascii_lowercase();
        let mut chars = lower.chars();
        let first = chars.next().expect("non-empty, checked above");
        if !first.is_ascii_alphanumeric() {
            return Err(reject(raw, IdentRejection::InvalidCharacter));
        }
        if !chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') {
            return Err(reject(raw, IdentRejection::InvalidCharacter));
        }

        Ok(Self(lower))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

fn reject(raw: &str, reason: IdentRejection) -> KernelError {
    KernelError::InvalidIdent {
        raw: raw.to_string(),
        reason,
    }
}

impl AsRef<str> for Ident {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl TryFrom<&str> for Ident {
    type Error = KernelError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Ident::parse(value)
    }
}

impl TryFrom<String> for Ident {
    type Error = KernelError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Ident::parse(&value)
    }
}

impl Serialize for Ident {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Ident {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        Ident::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_typical_identifiers() {
        for raw in ["prod", "dc-use1", "env_2", "A", "Dome-Prod01"] {
            assert!(Ident::parse(raw).is_ok(), "expected {raw:?} to be valid");
        }
    }

    #[test]
    fn lowercases_on_parse() {
        assert_eq!(Ident::parse("PROD").unwrap().as_str(), "prod");
    }

    #[test]
    fn rejects_empty() {
        assert_eq!(
            Ident::parse("").unwrap_err(),
            KernelError::InvalidIdent {
                raw: "".into(),
                reason: IdentRejection::Empty
            }
        );
    }

    #[test]
    fn rejects_path_traversal() {
        for raw in ["..", ".", "../etc", "a/../b", "a/b", "a\\b", "/etc/passwd"] {
            assert!(
                Ident::parse(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    #[test]
    fn rejects_absolute_and_hidden() {
        assert!(Ident::parse("/abs").is_err());
        assert!(Ident::parse(".hidden").is_err());
        assert!(Ident::parse("#reserved").is_err());
    }

    #[test]
    fn rejects_colon_and_nul() {
        assert!(Ident::parse("type:name").is_err());
        assert!(Ident::parse("a\0b").is_err());
    }

    #[test]
    fn rejects_too_long() {
        let raw = "a".repeat(MAX_IDENT_LEN + 1);
        assert!(Ident::parse(&raw).is_err());
    }

    #[test]
    fn rejects_invalid_characters() {
        for raw in ["has space", "quote\"", "semi;colon", "emoji😀"] {
            assert!(
                Ident::parse(raw).is_err(),
                "expected {raw:?} to be rejected"
            );
        }
    }

    #[test]
    fn serde_roundtrip() {
        let ident = Ident::parse("prod").unwrap();
        let json = serde_json::to_string(&ident).unwrap();
        assert_eq!(json, "\"prod\"");
        let back: Ident = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ident);
    }

    #[test]
    fn serde_rejects_invalid_on_deserialize() {
        let err = serde_json::from_str::<Ident>("\"../etc\"");
        assert!(err.is_err());
    }
}
