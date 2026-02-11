use super::EngineHandle;
use crate::error::NaviscopeError;
use crate::features::query::QueryEngine;
use async_trait::async_trait;
use naviscope_api::{graph, models};

#[async_trait]
impl graph::GraphService for EngineHandle {
    async fn query(&self, query: &models::GraphQuery) -> graph::Result<models::QueryResult> {
        let graph = self.graph().await;
        let query_clone = query.clone();
        let handle = self.clone();

        let result = tokio::task::spawn_blocking(
            move || -> Result<crate::features::query::QueryResult, NaviscopeError> {
                let conventions = (*handle.naming_conventions()).clone();
                let engine =
                    QueryEngine::new(&graph, |lang| handle.get_node_presenter(lang), conventions);
                engine.execute(&query_clone)
            },
        )
        .await
        .map_err(|e| graph::GraphError::Internal(e.to_string()))?
        .map_err(|e| graph::GraphError::Internal(e.to_string()))?;

        Ok(models::QueryResult {
            nodes: result.nodes,
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

    async fn get_node_display(&self, fqn: &str) -> graph::Result<Option<models::DisplayGraphNode>> {
        let query = models::GraphQuery::Cat {
            fqn: fqn.to_string(),
        };
        let result = self.query(&query).await?;
        Ok(result.nodes.into_iter().next())
    }
}
