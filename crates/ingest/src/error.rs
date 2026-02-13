use thiserror::Error;

#[derive(Debug, Error)]
pub enum IngestError {
    #[error("invalid message: {0}")]
    InvalidMessage(String),
    #[error("dependency resolution failed: {0}")]
    Dependency(String),
    #[error("execution failed: {0}")]
    Execution(String),
    #[error("commit failed: {0}")]
    Commit(String),
    #[error("storage failed: {0}")]
    Storage(String),
}
