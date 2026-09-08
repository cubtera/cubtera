//! Errors for `cubtera-exec`.

/// Failures from workspace I/O, process execution, or version resolution.
#[derive(Debug, thiserror::Error)]
pub enum ExecError {
    /// Filesystem I/O failed.
    #[error("io error: {0}")]
    Io(String),

    /// A path escaped the [`crate::Workspace`]'s root - should be
    /// unreachable given [`cubtera_kernel::SafeSegment`]'s guarantees, but
    /// kept as a defensive, explicit error rather than a panic.
    #[error("path escapes workspace root: {0}")]
    Containment(String),

    /// Spawning or waiting for a child process failed.
    #[error("process execution failed: {0}")]
    Process(String),

    /// A `RunnerStrategy` could not resolve a usable binary (missing on
    /// `PATH`, version mismatch, or download failure).
    #[error("version resolution failed: {0}")]
    Version(String),

    /// Something the strategy expected in the workspace wasn't there (e.g.
    /// no `.sh` script for `BashRunner`).
    #[error("not found: {0}")]
    NotFound(String),
}

/// Convenience alias.
pub type ExecResult<T> = Result<T, ExecError>;

impl From<std::io::Error> for ExecError {
    fn from(e: std::io::Error) -> Self {
        ExecError::Io(e.to_string())
    }
}
