use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::project::source::Language;
use smol_str::SmolStr;
use std::path::Path;
use std::sync::Arc;

// Re-export types from API
pub use naviscope_api::models::{EdgeType, GraphEdge, NodeKind, Range};

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct GraphNode {
    // --- Identity ---
    #[serde(with = "crate::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub id: Arc<str>, // Unique Identifier (FQN)
    #[schemars(with = "String")]
    pub name: SmolStr, // Short display name
    pub kind: NodeKind, // Abstract categorization
    #[serde(with = "crate::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub lang: Arc<str>, // Language identifier ("java", "rust", "buildfile")

    // --- Physical Location ---
    pub location: Option<NodeLocation>,

    // --- Extension Point ---
    #[serde(default = "empty_metadata")]
    pub metadata: serde_json::Value,
}

fn empty_metadata() -> serde_json::Value {
    serde_json::Value::Null
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct NodeLocation {
    #[serde(with = "crate::util::serde_arc_path")]
    #[schemars(with = "String")]
    pub path: Arc<Path>,
    pub range: Range,
    pub selection_range: Option<Range>, // Range of the identifier
}

impl GraphNode {
    pub fn language(&self) -> Language {
        match self.lang.as_ref() {
            "java" => Language::Java,
            _ => Language::BuildFile,
        }
    }

    pub fn fqn(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> NodeKind {
        self.kind.clone()
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.location.as_ref().map(|l| l.path.as_ref())
    }

    pub fn range(&self) -> Option<&Range> {
        self.location.as_ref().map(|l| &l.range)
    }

    pub fn name_range(&self) -> Option<&Range> {
        self.location
            .as_ref()
            .and_then(|l| l.selection_range.as_ref())
    }

    pub fn to_api(&self) -> naviscope_api::models::GraphNode {
        naviscope_api::models::GraphNode {
            id: self.id.to_string(),
            name: self.name.to_string(),
            kind: self.kind.clone(),
            lang: self.lang.to_string(),
            location: self
                .location
                .as_ref()
                .map(|l| naviscope_api::models::SymbolLocation {
                    path: l.path.to_path_buf(),
                    range: l.range.clone(),
                    fqn: self.id.to_string(),
                }),
            metadata: self.metadata.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GraphOp {
    /// Add or update a node
    AddNode {
        #[serde(with = "crate::util::serde_arc_str")]
        id: Arc<str>,
        data: GraphNode,
    },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        #[serde(with = "crate::util::serde_arc_str")]
        from_id: Arc<str>,
        #[serde(with = "crate::util::serde_arc_str")]
        to_id: Arc<str>,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath {
        #[serde(with = "crate::util::serde_arc_path")]
        path: Arc<Path>,
    },
    /// Update the reference index for a specific file
    UpdateIdentifiers {
        #[serde(with = "crate::util::serde_arc_path")]
        path: Arc<Path>,
        identifiers: Vec<SmolStr>,
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
    pub nodes: std::collections::HashMap<Arc<str>, GraphNode>,
    /// All unique identifier tokens in this file
    pub identifiers: Vec<SmolStr>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            nodes: std::collections::HashMap::new(),
            identifiers: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: Arc<str>, data: GraphNode) {
        self.nodes.insert(id.clone(), data.clone());
        self.ops.push(GraphOp::AddNode { id, data });
    }

    pub fn add_edge(&mut self, from_id: Arc<str>, to_id: Arc<str>, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
    }
}
