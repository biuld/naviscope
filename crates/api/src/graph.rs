pub use crate::models::graph::{GraphQuery, QueryResult};
use async_trait::async_trait;

#[derive(Debug, thiserror::Error)]
pub enum GraphError {
    #[error("Internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, GraphError>;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
}

#[async_trait]
pub trait GraphService: Send + Sync {
    async fn query(&self, query: &GraphQuery) -> Result<QueryResult>;
    async fn get_stats(&self) -> Result<GraphStats>;

    /// Get a fully hydrated display node by its FQN.
    async fn get_node_display(&self, fqn: &str) -> Result<Option<crate::models::DisplayGraphNode>>;
}
