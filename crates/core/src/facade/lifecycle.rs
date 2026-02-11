use super::EngineHandle;
use crate::error::NaviscopeError;
use async_trait::async_trait;
use naviscope_api::lifecycle::{EngineLifecycle, EngineWatchHandle};
use naviscope_api::{ApiError, ApiResult};
use std::sync::Arc;

struct WatchHandle {
    token: tokio_util::sync::CancellationToken,
}

impl EngineWatchHandle for WatchHandle {
    fn stop(&self) {
        self.token.cancel();
    }
}

#[async_trait]
impl EngineLifecycle for EngineHandle {
    async fn rebuild(&self) -> ApiResult<()> {
        self.engine
            .rebuild()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))
    }

    async fn load(&self) -> ApiResult<bool> {
        self.engine
            .load()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))
    }

    async fn save(&self) -> ApiResult<()> {
        self.engine
            .save()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))
    }

    async fn refresh(&self) -> ApiResult<()> {
        self.engine
            .refresh()
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))
    }

    async fn start_watch(&self) -> ApiResult<Arc<dyn EngineWatchHandle>> {
        let watch_token = tokio_util::sync::CancellationToken::new();
        self.engine
            .clone()
            .start_watch_with_token(watch_token.clone())
            .await
            .map_err(|e: NaviscopeError| ApiError::Internal(e.to_string()))?;

        Ok(Arc::new(WatchHandle { token: watch_token }))
    }

    async fn clear_index(&self) -> ApiResult<()> {
        self.engine
            .clear_project_index()
            .await
            .map_err(|e: NaviscopeError| ApiError::Internal(e.to_string()))
    }
}
