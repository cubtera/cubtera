//! Cubtera Domain Layer
//!
//! This crate contains pure business logic with zero external dependencies.
//! All types here are framework-agnostic and can be used anywhere.

mod access;
mod dimension;
mod error;
mod manifest;
mod materialization;
mod runner;
mod schema;
mod unit;

pub use access::*;
pub use dimension::*;
pub use error::*;
pub use manifest::*;
pub use materialization::*;
pub use runner::*;
pub use schema::*;
pub use unit::*;

/// Re-exported so downstream crates use a single `Value` type across all layers.
/// The domain has no I/O dependency of its own; `serde_json` is treated as a data
/// format (like `String` or `Vec<u8>`), not an infrastructure concern.
pub use serde_json::Value;
