#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Unsupported capability: {capability} for language {language}")]
    UnsupportedCapability {
        capability: &'static str,
        language: String,
    },
    #[error("Not found: {0}")]
    NotFound(String),
    #[error("Invalid argument: {0}")]
    InvalidArgument(String),
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type ApiResult<T> = std::result::Result<T, ApiError>;
