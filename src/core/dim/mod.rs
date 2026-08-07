// Core dimension module providing dimension management functionality
pub mod data;
pub mod error;
pub mod dim;
pub mod builder;
pub mod file_ops;

#[cfg(test)]
pub mod tests;

// Re-export main types for backward compatibility and clean public API
pub use dim::Dim;
pub use builder::DimBuilder;
pub use error::{DimError, DimResult, DimResultExt};

// Re-export data types for convenience
pub use data::*; 