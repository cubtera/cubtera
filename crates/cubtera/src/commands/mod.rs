//! CLI commands

pub mod apply;
pub mod config;
pub mod drift;
pub mod explain;
pub mod fleet;
pub mod im;
pub mod log;
pub mod migrate;
pub mod plan;
pub mod run;
pub mod run_support;
pub mod state;
pub mod validate;

/// Cross-cutting CLI options every command needs, distinct from `Config`
/// (which is inventory/runner configuration, not presentation).
pub struct Ctx {
    /// Emit JSON instead of human-readable text
    pub json: bool,
}
