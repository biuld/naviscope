use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

// Re-export core models from API layer for internal use
pub use super::metadata::{IndexMetadata, SymbolInterner};
pub use naviscope_api::models::{
    DisplayGraphNode, DisplaySymbolLocation, EdgeType, EmptyMetadata, GraphEdge, GraphNode,
    InternedLocation, Language, NodeKind, NodeMetadata, QueryResultEdge, Range, SymbolLocation,
};

pub type NodeLocation = SymbolLocation;

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum GraphOp {
    /// Add or update a node
    AddNode {
        #[serde(skip)]
        data: Option<crate::ingest::parser::IndexNode>,
    },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        from_id: crate::ingest::parser::NodeId,
        to_id: crate::ingest::parser::NodeId,
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
        metadata: crate::model::source::SourceFile,
    },
}

/// Result of resolving a single file
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this file into the graph
    pub ops: Vec<GraphOp>,
    /// Fast access to nodes being added in this unit (before interning)
    pub nodes:
        std::collections::HashMap<crate::ingest::parser::NodeId, crate::ingest::parser::IndexNode>,
    /// All unique identifier tokens in this file
    pub identifiers: Vec<String>,
    /// Naming convention for upgrading FQNs
    pub naming_convention: Option<std::sync::Arc<dyn naviscope_plugin::NamingConvention>>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            nodes: std::collections::HashMap::new(),
            identifiers: Vec::new(),
            naming_convention: None,
        }
    }

    pub fn add_node(&mut self, data: crate::ingest::parser::IndexNode) {
        self.nodes.insert(data.id.clone(), data.clone());
        self.ops.push(GraphOp::AddNode { data: Some(data) });
    }

    pub fn add_edge(
        &mut self,
        from_id: crate::ingest::parser::NodeId,
        to_id: crate::ingest::parser::NodeId,
        edge: GraphEdge,
    ) {
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
