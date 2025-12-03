//! Cubtera Application Layer
//!
//! This crate contains:
//! - Ports (traits) for external dependencies
//! - Application services (use cases)
//! - Composition root (App)

pub mod error;
pub mod ports;
pub mod services;

mod app;

pub use app::App;
pub use error::*;

