use super::EngineHandle;
use crate::error::NaviscopeError;
use async_trait::async_trait;
use naviscope_api::{graph, models};

#[async_trait]
impl graph::GraphService for EngineHandle {
    async fn query(&self, query: &models::GraphQuery) -> graph::Result<models::QueryResult> {
        let graph = self.graph().await;
        let query_clone = query.clone(); // Clone for 'static lifetime in spawn_blocking

        let result = tokio::task::spawn_blocking(
            move || -> Result<crate::query::QueryResult, NaviscopeError> {
                let engine = crate::query::QueryEngine::new(graph);
                engine.execute(&query_clone)
            },
        )
        .await
        .map_err(|e| graph::GraphError::Internal(e.to_string()))?
        .map_err(|e| graph::GraphError::Internal(e.to_string()))?;

        // Hydrate nodes before returning to provide rich information to upper layers
        let hydrated_nodes = result
            .nodes
            .into_iter()
            .map(|node| self.hydrate_node(node))
            .collect();

        Ok(models::QueryResult {
            nodes: hydrated_nodes,
            edges: result.edges,
        })
    }

    async fn get_stats(&self) -> graph::Result<graph::GraphStats> {
        let graph = self.graph().await;
        Ok(graph::GraphStats {
            node_count: graph.topology().node_count(),
            edge_count: graph.topology().edge_count(),
        })
    }
}
