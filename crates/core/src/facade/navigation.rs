use crate::facade::EngineHandle;
use crate::features::navigation::NavigationEngine;
use async_trait::async_trait;
use naviscope_api::ApiResult;
use naviscope_api::navigation::{NavigationService, ResolveResult};

#[async_trait]
impl NavigationService for EngineHandle {
    async fn resolve_path(
        &self,
        target: &str,
        current_context: Option<&str>,
    ) -> ApiResult<ResolveResult> {
        let graph = self.graph().await;
        let conventions = (*self.naming_conventions()).clone();
        let engine = NavigationEngine::new(&graph, conventions);
        engine.resolve_path(target, current_context)
    }

    async fn get_completion_candidates(
        &self,
        prefix: &str,
        limit: usize,
    ) -> ApiResult<Vec<String>> {
        let graph = self.graph().await;
        let conventions = (*self.naming_conventions()).clone();
        let engine = NavigationEngine::new(&graph, conventions);
        engine.get_completion_candidates(prefix, limit)
    }
}
