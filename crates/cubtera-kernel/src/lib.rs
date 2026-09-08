//! Cubtera v3 kernel: validated identity types and content hashing.
//!
//! This crate is the single place raw strings from CLI args, TOML manifests,
//! inventory records, or HTTP bodies are allowed to become identity. Every
//! other crate in the workspace receives [`Ident`]/[`DimRef`]/[`InstanceId`]
//! values at its public boundary, never a bare `&str` - so "validate, then
//! use a different unchecked path" (the shape of the v2 path-escape bug
//! class) is not expressible.
//!
//! Zero I/O, zero async, zero filesystem/process/network access - same rule
//! v2 applied to `cubtera-domain`, applied here to the layer everything else
//! (including `cubtera-domain`'s eventual replacement, `cubtera-model`)
//! depends on.

mod dim_ref;
mod digest;
mod error;
mod ident;
mod instance_id;
mod safe_segment;

pub use dim_ref::DimRef;
pub use digest::Digest;
pub use error::KernelError;
pub use ident::Ident;
pub use instance_id::InstanceId;
pub use safe_segment::SafeSegment;

/// Result type for every fallible operation in this crate.
pub type KernelResult<T> = Result<T, KernelError>;
