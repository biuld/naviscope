use super::EngineHandle;
use crate::error::NaviscopeError;
use async_trait::async_trait;
use naviscope_api::lifecycle::{EngineError, EngineLifecycle};

#[async_trait]
impl EngineLifecycle for EngineHandle {
    async fn rebuild(&self) -> naviscope_api::lifecycle::EngineResult<()> {
        self.engine
            .rebuild()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn load(&self) -> naviscope_api::lifecycle::EngineResult<bool> {
        self.engine
            .load()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn save(&self) -> naviscope_api::lifecycle::EngineResult<()> {
        self.engine
            .save()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn refresh(&self) -> naviscope_api::lifecycle::EngineResult<()> {
        self.engine
            .refresh()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn watch(&self) -> naviscope_api::lifecycle::EngineResult<()> {
        self.engine
            .clone()
            .watch()
            .await
            .map_err(|e: NaviscopeError| EngineError::Internal(e.to_string()))
    }

    async fn clear_index(&self) -> naviscope_api::lifecycle::EngineResult<()> {
        self.engine
            .clear_project_index()
            .await
            .map_err(|e: NaviscopeError| EngineError::Internal(e.to_string()))
    }
}
