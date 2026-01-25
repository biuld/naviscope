use crate::model::graph::{GraphEdge, GraphNode};
use serde::{Deserialize, Serialize};

/// A structured edge in the query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResultEdge {
    pub from: String,
    pub to: String,
    pub data: GraphEdge,
}

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
