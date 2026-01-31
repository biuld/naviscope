use crate::model::graph::{GraphEdge, GraphNode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// A structured edge in the query result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResultEdge {
    #[serde(with = "crate::util::serde_arc_str")]
    pub from: Arc<str>,
    #[serde(with = "crate::util::serde_arc_str")]
    pub to: Arc<str>,
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
