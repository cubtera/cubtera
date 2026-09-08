use crate::error::KernelError;
use crate::ident::Ident;
use serde::{Deserialize, Serialize};
use std::fmt;

/// A validated `type:name` reference - a dimension (`dome:prod`) or
/// extension (`index:0`) value. Both halves are already-valid [`Ident`]s,
/// so a `DimRef` can never carry a path-traversal payload in either
/// position.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DimRef {
    pub dim_type: Ident,
    pub name: Ident,
}

impl DimRef {
    pub fn new(dim_type: Ident, name: Ident) -> Self {
        Self { dim_type, name }
    }

    /// Parse a `"type:name"` string. Splits on the *first* `:` only, so the
    /// name half may not itself contain `:` (enforced by [`Ident::parse`]
    /// rejecting `:` unconditionally - there is exactly one place a `:` is
    /// meaningful, the separator between the two halves).
    pub fn parse(raw: &str) -> Result<Self, KernelError> {
        let (dim_type, name) = raw
            .split_once(':')
            .ok_or_else(|| KernelError::InvalidDimRef {
                raw: raw.to_string(),
            })?;
        Ok(Self {
            dim_type: Ident::parse(dim_type)?,
            name: Ident::parse(name)?,
        })
    }

    /// The canonical `"type:name"` string form - used as a single path
    /// component (matches v2's `DimensionRef::key()` on-disk convention)
    /// and as the human-readable form in CLI/API output.
    pub fn key(&self) -> String {
        format!("{}:{}", self.dim_type, self.name)
    }
}

impl fmt::Display for DimRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.dim_type, self.name)
    }
}

impl TryFrom<&str> for DimRef {
    type Error = KernelError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        DimRef::parse(value)
    }
}

impl Serialize for DimRef {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.key())
    }
}

impl<'de> Deserialize<'de> for DimRef {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        DimRef::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_valid_refs() {
        let r = DimRef::parse("dome:prod").unwrap();
        assert_eq!(r.dim_type.as_str(), "dome");
        assert_eq!(r.name.as_str(), "prod");
        assert_eq!(r.key(), "dome:prod");
    }

    #[test]
    fn rejects_missing_separator() {
        assert!(DimRef::parse("dome").is_err());
    }

    #[test]
    fn rejects_traversal_in_either_half() {
        for raw in ["../etc:prod", "dome:../etc", "dome:name/with/slash"] {
            assert!(DimRef::parse(raw).is_err(), "expected {raw:?} rejected");
        }
    }

    #[test]
    fn rejects_extra_colon_in_name() {
        // Only the first ':' is a separator; a second ':' would have to be
        // part of `name`, which `Ident::parse` unconditionally rejects.
        assert!(DimRef::parse("dome:pro:d").is_err());
    }

    #[test]
    fn ordering_is_by_type_then_name() {
        let a = DimRef::parse("dc:a").unwrap();
        let b = DimRef::parse("dome:a").unwrap();
        assert!(a < b, "dc sorts before dome");
    }
}
