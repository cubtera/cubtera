#[derive(Debug, thiserror::Error)]
pub enum SourceError {
    #[error("io error: {0}")]
    Io(String),
    #[error("git command failed: {0}")]
    Git(String),
    #[error("invalid module reference {0:?}: {1}")]
    InvalidReference(String, String),
    #[error("path not found: {0:?}")]
    NotFound(String),
}

impl SourceError {
    pub fn io(msg: impl Into<String>) -> Self {
        Self::Io(msg.into())
    }
    pub fn git(msg: impl Into<String>) -> Self {
        Self::Git(msg.into())
    }
}
