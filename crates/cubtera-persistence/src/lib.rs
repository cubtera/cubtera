//! Cubtera Persistence Layer
//!
//! Repository implementations for different storage backends.

#[cfg(feature = "fs")]
pub mod fs;

#[cfg(feature = "mongodb")]
pub mod mongodb;

mod factory;

pub use factory::*;

