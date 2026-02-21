use thiserror::Error;

/// Core error type for gAgent operations.
#[derive(Debug, Error)]
pub enum GagentError {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Bootstrap error: {0}")]
    Bootstrap(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("Tool error: {0}")]
    Tool(String),

    #[error("Session error: {0}")]
    Session(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("HTTP error: {0}")]
    Http(String),

    #[error("Timeout after {0} seconds")]
    Timeout(u64),

    #[error("Path not allowed: {0}")]
    PathNotAllowed(String),

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, GagentError>;
