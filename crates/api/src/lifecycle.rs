use crate::models::Language;
use crate::plugin::LanguageFeatureProvider;
use async_trait::async_trait;
use std::sync::Arc;

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

    /// Get a feature provider for a specific language
    fn get_feature_provider(&self, language: Language) -> Option<Arc<dyn LanguageFeatureProvider>>;
}
