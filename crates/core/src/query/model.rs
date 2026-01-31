use crate::model::GraphNode;
pub use crate::model::QueryResultEdge;
use serde::{Deserialize, Serialize};

/// The result of a query execution, representing a subgraph
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryResult {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<QueryResultEdge>,
}

impl QueryResult {
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<QueryResultEdge>) -> Self {
        Self { nodes, edges }
    }

    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}
