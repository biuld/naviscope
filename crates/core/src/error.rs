use thiserror::Error;

#[derive(Error, Debug)]
pub enum NaviscopeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Plugin error: {0}")]
    Plugin(String),
    #[error("Unknown error")]
    Unknown,
}

impl From<Box<dyn std::error::Error + Send + Sync>> for NaviscopeError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        NaviscopeError::Plugin(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, NaviscopeError>;
