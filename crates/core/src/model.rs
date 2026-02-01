use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// Re-export core models from API layer for internal use
pub use naviscope_api::models::{
    DisplayGraphNode, DisplaySymbolLocation, EdgeType, GraphEdge, GraphNode, InternedLocation,
    Language, NodeKind, QueryResultEdge, Range, SymbolLocation,
};

pub type NodeLocation = SymbolLocation;

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GraphOp {
    /// Add or update a node
    AddNode {
        data: DisplayGraphNode,
    },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        #[serde(with = "naviscope_api::models::util::serde_arc_str")]
        from_id: Arc<str>,
        #[serde(with = "naviscope_api::models::util::serde_arc_str")]
        to_id: Arc<str>,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath {
        #[serde(with = "naviscope_api::models::util::serde_arc_path")]
        path: Arc<Path>,
    },
    /// Update the reference index for a specific file
    UpdateIdentifiers {
        #[serde(with = "naviscope_api::models::util::serde_arc_path")]
        path: Arc<Path>,
        identifiers: Vec<String>,
    },
    /// Update file metadata (hash, mtime)
    UpdateFile {
        metadata: crate::project::source::SourceFile,
    },
}

/// Result of resolving a single file
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this file into the graph
    pub ops: Vec<GraphOp>,
    /// Fast access to nodes being added in this unit
    pub nodes: std::collections::HashMap<Arc<str>, DisplayGraphNode>,
    /// All unique identifier tokens in this file
    pub identifiers: Vec<String>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            nodes: std::collections::HashMap::new(),
            identifiers: Vec::new(),
        }
    }

    pub fn add_node(&mut self, data: DisplayGraphNode) {
        self.nodes.insert(Arc::from(data.id.as_str()), data.clone());
        self.ops.push(GraphOp::AddNode { data });
    }

    pub fn add_edge(&mut self, from_id: Arc<str>, to_id: Arc<str>, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
    }
}

pub mod util {
    pub fn line_col_at_to_offset(content: &str, line: usize, col: usize) -> Option<usize> {
        let mut offset = 0;
        for (i, l) in content.lines().enumerate() {
            if i == line {
                if col <= l.len() {
                    return Some(offset + col);
                }
                return None;
            }
            offset += l.len() + 1; // +1 for newline
        }
        None
    }
}
