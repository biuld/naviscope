use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

#[async_trait]
pub trait EngineLifecycle: Send + Sync {
    /// Rebuild the index from scratch
    async fn rebuild(&self) -> EngineResult<()>;

    /// Load the index from disk
    async fn load(&self) -> EngineResult<bool>;

    /// Save the index to disk
    async fn save(&self) -> EngineResult<()>;

    /// Refresh the index (find new files, etc.)
    async fn refresh(&self) -> EngineResult<()>;

    /// Watch for filesystem changes
    async fn watch(&self) -> EngineResult<()>;

    /// Clear the index for the current project
    async fn clear_index(&self) -> EngineResult<()>;
}
