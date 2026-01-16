use thiserror::Error;

#[derive(Error, Debug)]
pub enum NaviscopeError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON serialization/deserialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Parsing error: {0}")]
    Parsing(String),
    #[error("Unknown error")]
    Unknown,
}

pub type Result<T> = std::result::Result<T, NaviscopeError>;
