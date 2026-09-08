//! CLI exit codes
//!
//! v1 exits `1` for every error and `0` for "access denied" (silently, via
//! `process::exit(0)` deep in `core::unit`). v2 made access denial a real,
//! non-zero outcome instead; v3 (this crate has no `cubtera-core`/
//! `cubtera-domain`/`cubtera-persistence`/`cubtera-runners` dependency
//! left at all - see `commands/run.rs`'s doc comment) keeps the same
//! mapping, now sourced from `cubtera_app::AppError` alone. Scripts that
//! need fine-grained detail should parse `--json` output, not branch on
//! exit codes.
//!
//! Command functions return `Box<dyn Error>` (they thread through several
//! unrelated error types via `?`), so exit code selection downcasts back to
//! the specific types the CLI cares about at the one place that needs it:
//! here, in `main`.

use cubtera_app::AppError;
use cubtera_config::ConfigError;

pub const EXIT_GENERAL_ERROR: i32 = 1;
pub const EXIT_ACCESS_DENIED: i32 = 3;
pub const EXIT_NOT_FOUND: i32 = 4;
pub const EXIT_VALIDATION: i32 = 5;
pub const EXIT_CONFIG: i32 = 6;
/// `cubtera drift` found at least one `PackageDrifted`/`Orphaned` instance.
/// Not an error in the usual sense (the command itself ran fine), but a
/// distinct, deliberately non-zero signal for CI-style drift checks,
/// separate from every other code above so it can never be confused with
/// an actual failure.
pub const EXIT_DRIFT_DETECTED: i32 = 7;

/// Map a top-level command error to a process exit code.
pub fn exit_code_for(err: &(dyn std::error::Error + 'static)) -> i32 {
    if let Some(app_err) = err.downcast_ref::<AppError>() {
        return exit_code_for_app_error(app_err);
    }
    if err.downcast_ref::<ConfigError>().is_some() {
        return EXIT_CONFIG;
    }
    EXIT_GENERAL_ERROR
}

fn exit_code_for_app_error(err: &AppError) -> i32 {
    match err {
        AppError::AccessDenied(_) => EXIT_ACCESS_DENIED,
        AppError::NotFound { .. } => EXIT_NOT_FOUND,
        AppError::Validation(_) | AppError::Model(_) => EXIT_VALIDATION,
        AppError::Backend(_) => EXIT_GENERAL_ERROR,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_denied_maps_to_its_own_code() {
        let err: Box<dyn std::error::Error> = Box::new(AppError::access_denied("nope".to_string()));
        assert_eq!(exit_code_for(err.as_ref()), EXIT_ACCESS_DENIED);
    }

    #[test]
    fn not_found_maps_to_its_own_code() {
        let err: Box<dyn std::error::Error> = Box::new(AppError::not_found("unit", "foo"));
        assert_eq!(exit_code_for(err.as_ref()), EXIT_NOT_FOUND);
    }

    #[test]
    fn config_error_maps_to_config_code() {
        let err: Box<dyn std::error::Error> = Box::new(ConfigError::Missing("org".to_string()));
        assert_eq!(exit_code_for(err.as_ref()), EXIT_CONFIG);
    }

    #[test]
    fn unknown_error_falls_back_to_general() {
        let err: Box<dyn std::error::Error> = Box::new(std::fmt::Error);
        assert_eq!(exit_code_for(err.as_ref()), EXIT_GENERAL_ERROR);
    }
}
