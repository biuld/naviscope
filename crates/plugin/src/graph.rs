use crate::model::{IndexNode, SourceFile};
use crate::naming::NamingConvention;
use naviscope_api::models::graph::{EdgeType, GraphEdge, GraphNode};
use naviscope_api::models::symbol::{FqnId, FqnReader, NodeId, Symbol};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum GraphOp {
    /// Add or update a node
    AddNode { data: Option<IndexNode> },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        from_id: NodeId,
        to_id: NodeId,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath { path: Arc<Path> },
    /// Update the reference index for a specific file
    UpdateIdentifiers {
        path: Arc<Path>,
        identifiers: Vec<String>,
    },
    /// Update file metadata (hash, mtime)
    UpdateFile { metadata: SourceFile },
    /// Update the asset routing table: Prefix -> Asset Path
    UpdateAssetRoutes {
        routes: HashMap<String, Vec<std::path::PathBuf>>,
    },
}

/// Result of resolving a single file or unit
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this unit into the graph
    pub ops: Vec<GraphOp>,
    /// Fast access to nodes being added in this unit (before interning)
    pub nodes: HashMap<NodeId, IndexNode>,
    /// All unique identifier tokens in this unit
    pub identifiers: Vec<String>,
    /// Naming convention for upgrading FQNs
    pub naming_convention: Option<Arc<dyn NamingConvention>>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            nodes: HashMap::new(),
            identifiers: Vec::new(),
            naming_convention: None,
        }
    }

    pub fn add_node(&mut self, data: IndexNode) {
        self.nodes.insert(data.id.clone(), data.clone());
        self.ops.push(GraphOp::AddNode { data: Some(data) });
    }

    pub fn add_edge(&mut self, from_id: NodeId, to_id: NodeId, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
    }
}

/// Unified interface for reading code graph information.
/// This trait isolates plugins from the core's graph storage implementation.
pub trait CodeGraph: Send + Sync {
    /// Resolve an FQN string to one or more internal FQN identifiers.
    fn resolve_fqn(&self, fqn: &str) -> Vec<FqnId>;

    /// Get the FQN identifier for a node at a specific position.
    fn get_node_at(&self, path: &Path, line: usize, col: usize) -> Option<FqnId>;

    /// Resolve an atom (Symbol) to its string representation.
    fn resolve_atom(&self, atom: Symbol) -> &str;

    /// Get the reader for FQNs.
    fn fqns(&self) -> &dyn FqnReader;

    /// Get a node by its FQN identifier.
    fn get_node(&self, id: FqnId) -> Option<GraphNode>;

    /// Get neighbors of a node in the specified direction, optionally filtered by edge type.
    fn get_neighbors(
        &self,
        id: FqnId,
        direction: Direction,
        edge_type: Option<EdgeType>,
    ) -> Vec<FqnId>;
}

pub struct EmptyCodeGraph;

impl CodeGraph for EmptyCodeGraph {
    fn resolve_fqn(&self, _fqn: &str) -> Vec<FqnId> {
        vec![]
    }
    fn get_node_at(&self, _path: &Path, _line: usize, _col: usize) -> Option<FqnId> {
        None
    }
    fn resolve_atom(&self, _atom: Symbol) -> &str {
        ""
    }
    fn fqns(&self) -> &dyn FqnReader {
        self
    }
    fn get_node(&self, _id: FqnId) -> Option<GraphNode> {
        None
    }
    fn get_neighbors(
        &self,
        _id: FqnId,
        _direction: Direction,
        _edge_type: Option<EdgeType>,
    ) -> Vec<FqnId> {
        vec![]
    }
}

impl FqnReader for EmptyCodeGraph {
    fn resolve_node(&self, _id: FqnId) -> Option<naviscope_api::models::symbol::FqnNode> {
        None
    }
    fn resolve_atom(&self, _atom: Symbol) -> &str {
        ""
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Incoming,
    Outgoing,
}
