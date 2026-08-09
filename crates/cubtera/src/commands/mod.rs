//! CLI commands

pub mod config;
pub mod im;
pub mod log;
pub mod run;
pub mod state;

/// Cross-cutting CLI options every command needs, distinct from `Config`
/// (which is inventory/runner configuration, not presentation).
pub struct Ctx {
    /// Emit JSON instead of human-readable text
    pub json: bool,
}
