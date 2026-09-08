use std::fmt;

/// Errors raised while parsing/validating kernel identity types. Every
/// variant carries enough context to explain *why* a value was rejected -
/// callers at the CLI/API/MCP edges map this straight to a 400/validation
/// error, never a panic.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KernelError {
    #[error("invalid identifier {raw:?}: {reason}")]
    InvalidIdent { raw: String, reason: IdentRejection },

    #[error("invalid dimension reference {raw:?}: expected \"type:name\"")]
    InvalidDimRef { raw: String },

    #[error("invalid path segment {raw:?}: {reason}")]
    InvalidSegment { raw: String, reason: IdentRejection },

    #[error("instance id has duplicate dimension type {dim_type:?} in {scope}: {first:?} and {second:?}")]
    DuplicateDimType {
        scope: &'static str,
        dim_type: String,
        first: String,
        second: String,
    },
}

/// Why an [`crate::Ident`]/[`crate::SafeSegment`] was rejected. Exhaustive on
/// purpose - every rejection reason is testable and message-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentRejection {
    Empty,
    TooLong,
    LeadingDot,
    LeadingHash,
    ContainsPathSeparator,
    ContainsParentRef,
    ContainsNul,
    ContainsColon,
    InvalidCharacter,
}

impl fmt::Display for IdentRejection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let msg = match self {
            Self::Empty => "must not be empty",
            Self::TooLong => "exceeds the maximum identifier length",
            Self::LeadingDot => "must not start with '.'",
            Self::LeadingHash => "must not start with '#'",
            Self::ContainsPathSeparator => "must not contain '/' or '\\'",
            Self::ContainsParentRef => "must not be '.' or '..'",
            Self::ContainsNul => "must not contain a NUL byte",
            Self::ContainsColon => "must not contain ':' (reserved for type:name references)",
            Self::InvalidCharacter => {
                "must contain only lowercase ascii letters, digits, '_' and '-'"
            }
        };
        f.write_str(msg)
    }
}
