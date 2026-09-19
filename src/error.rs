use thiserror::Error;

/// Errors surfaced to the MCP caller. Messages never contain the API key or
/// request bodies.
#[derive(Debug, Error)]
pub enum Error {
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
    #[error("not a git repository (or git unavailable): {0}")]
    NotARepo(String),
    #[error("git failed: {0}")]
    Git(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
