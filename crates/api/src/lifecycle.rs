use crate::ApiResult;
use async_trait::async_trait;

pub trait EngineWatchHandle: Send + Sync {
    fn stop(&self);
}

#[async_trait]
pub trait EngineLifecycle: Send + Sync {
    /// Rebuild the index from scratch
    async fn rebuild(&self) -> ApiResult<()>;

    /// Load the index from disk
    async fn load(&self) -> ApiResult<bool>;

    /// Save the index to disk
    async fn save(&self) -> ApiResult<()>;

    /// Refresh the index (find new files, etc.)
    async fn refresh(&self) -> ApiResult<()>;

    /// Watch for filesystem changes
    async fn start_watch(&self) -> ApiResult<std::sync::Arc<dyn EngineWatchHandle>>;

    /// Clear the index for the current project
    async fn clear_index(&self) -> ApiResult<()>;
}
