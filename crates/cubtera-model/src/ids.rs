use serde::{Deserialize, Serialize};
use std::fmt;

/// Opaque identifier for one [`crate::Run`]. `cubtera-model` only defines
/// the newtype and its wire format; minting a fresh one (typically a
/// ULID/UUID plus a clock) is an adapter/app concern, not a model one - this
/// crate stays zero I/O, including "zero randomness".
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct RunId(String);

/// Opaque identifier for one [`crate::Plan`]. Same shape/rationale as
/// [`RunId`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PlanId(String);

macro_rules! opaque_id {
    ($ty:ident) => {
        impl $ty {
            pub fn new(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $ty {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl From<String> for $ty {
            fn from(raw: String) -> Self {
                Self(raw)
            }
        }

        impl AsRef<str> for $ty {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

opaque_id!(RunId);
opaque_id!(PlanId);
