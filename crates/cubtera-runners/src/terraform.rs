//! Terraform runner
//!
//! Executes Terraform commands with automatic version management.

mod runner;
mod switch;

pub use runner::TerraformRunner;
