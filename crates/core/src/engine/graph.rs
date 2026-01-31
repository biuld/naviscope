//! Arc-wrapped immutable code graph
//!
//! The `CodeGraph` provides a cheap-to-clone, immutable view of the indexed codebase.
//! All data is wrapped in `Arc`, so cloning only increments a reference counter.

use crate::error::NaviscopeError;
use crate::model::{GraphEdge, GraphNode};
use crate::project::source::SourceFile;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

/// Immutable code graph (cheap to clone via Arc)
#[derive(Clone)]
pub struct CodeGraph {
    inner: std::sync::Arc<CodeGraphInner>,
}

/// Internal data structure (shared via Arc)
#[derive(Clone)]
pub struct CodeGraphInner {
    pub version: u32,
    pub topology: StableDiGraph<GraphNode, GraphEdge>,

    /// FQN -> NodeIndex mapping for fast lookup
    pub fqn_index: HashMap<Arc<str>, NodeIndex>,

    /// Simple name -> NodeIndices for symbol search
    pub name_index: HashMap<SmolStr, Vec<NodeIndex>>,

    /// File-level information: metadata and nodes contained in each file
    pub file_index: HashMap<Arc<Path>, FileEntry>,

    /// Reference Index: Token (e.g. Method Name) -> Files that contain this token.
    /// Used for fast "scouting" during reference discovery.
    pub reference_index: HashMap<SmolStr, Vec<Arc<Path>>>,
}

/// Metadata and nodes associated with a single source file
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub metadata: SourceFile,
    pub nodes: Vec<NodeIndex>,
}

impl CodeGraph {
    /// Create an empty graph
    pub fn empty() -> Self {
        Self {
            inner: std::sync::Arc::new(CodeGraphInner {
                version: crate::engine::CURRENT_VERSION,
                topology: StableDiGraph::new(),
                fqn_index: HashMap::new(),
                name_index: HashMap::new(),
                file_index: HashMap::new(),
                reference_index: HashMap::new(),
            }),
        }
    }

    /// Create graph from internal data
    pub(crate) fn from_inner(inner: CodeGraphInner) -> Self {
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }

    /// Create a builder for modifying this graph
    ///
    /// Note: This performs a deep copy, so it should only be called when
    /// building/updating the index, not during queries.
    pub fn to_builder(&self) -> super::CodeGraphBuilder {
        super::CodeGraphBuilder::from_inner((*self.inner).clone())
    }

    // ---- Read-only accessors ----

    /// Get the version number
    pub fn version(&self) -> u32 {
        self.inner.version
    }

    /// Get reference to the topology graph
    pub fn topology(&self) -> &StableDiGraph<GraphNode, GraphEdge> {
        &self.inner.topology
    }

    /// Get reference to the FQN index
    pub fn fqn_map(&self) -> &HashMap<Arc<str>, NodeIndex> {
        &self.inner.fqn_index
    }

    /// Get reference to the name index
    pub fn name_map(&self) -> &HashMap<SmolStr, Vec<NodeIndex>> {
        &self.inner.name_index
    }

    /// Get reference to the file index
    pub fn file_index(&self) -> &HashMap<Arc<Path>, FileEntry> {
        &self.inner.file_index
    }

    /// Get reference to the reference index
    pub fn reference_index(&self) -> &HashMap<SmolStr, Vec<Arc<Path>>> {
        &self.inner.reference_index
    }

    /// Find node index by FQN
    pub fn find_node(&self, fqn: &str) -> Option<NodeIndex> {
        self.inner.fqn_index.get(fqn).copied()
    }

    /// Get node data by index
    pub fn get_node(&self, idx: NodeIndex) -> Option<&GraphNode> {
        self.inner.topology.node_weight(idx)
    }

    /// Find node at a specific location in a file
    pub fn find_node_at(&self, path: &Path, line: usize, col: usize) -> Option<NodeIndex> {
        let entry = self.inner.file_index.get(path)?;

        for &idx in &entry.nodes {
            if let Some(node) = self.inner.topology.node_weight(idx) {
                let range_opt: Option<&crate::model::Range> = node.name_range();
                if let Some(range) = range_opt {
                    if range.contains(line, col) {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Find nodes matching a symbol resolution result
    pub fn find_matches_by_fqn(&self, fqn: &str) -> Vec<NodeIndex> {
        if let Some(&idx) = self.inner.fqn_index.get(fqn) {
            vec![idx]
        } else {
            vec![]
        }
    }

    /// Get the number of nodes
    pub fn node_count(&self) -> usize {
        self.inner.topology.node_count()
    }

    /// Get the number of edges
    pub fn edge_count(&self) -> usize {
        self.inner.topology.edge_count()
    }

    // ---- Serialization support ----

    /// Serialize to bytes for persistence
    pub fn serialize(&self) -> Result<Vec<u8>, NaviscopeError> {
        use super::storage::to_storage;
        let storage = to_storage(&self.inner);
        let bytes = rmp_serde::to_vec(&storage)
            .map_err(|e| NaviscopeError::Internal(format!("MSGPACK error: {}", e)))?;

        let compressed = zstd::encode_all(&bytes[..], 0)
            .map_err(|e| NaviscopeError::Internal(format!("Zstd compression failed: {}", e)))?;

        Ok(compressed)
    }

    /// Deserialize from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, NaviscopeError> {
        use super::storage::{StorageGraph, from_storage};

        // Decompress
        let decompressed = zstd::decode_all(bytes)
            .map_err(|e| NaviscopeError::Internal(format!("Zstd decompression failed: {}", e)))?;

        let storage: StorageGraph = rmp_serde::from_slice(&decompressed)
            .map_err(|e| NaviscopeError::Internal(format!("MSGPACK error: {}", e)))?;

        let inner = from_storage(storage);
        Ok(Self::from_inner(inner))
    }

    /// Save graph to JSON file (for debugging)
    pub fn save_to_json<P: AsRef<std::path::Path>>(&self, path: P) -> crate::error::Result<()> {
        use super::storage::to_storage;
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        let storage = to_storage(&self.inner);
        serde_json::to_writer_pretty(writer, &storage)
            .map_err(|e| crate::error::NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arc_clone_is_cheap() {
        let graph = CodeGraph::empty();

        // Arc clone should be O(1)
        let start = std::time::Instant::now();
        for _ in 0..100000 {
            let _clone = graph.clone();
        }
        let elapsed = start.elapsed();

        // 100K clones should be fast (< 10ms)
        assert!(
            elapsed.as_millis() < 10,
            "Arc clone should be cheap, took {:?}",
            elapsed
        );
    }

    #[test]
    fn test_empty_graph() {
        let graph = CodeGraph::empty();
        assert_eq!(graph.node_count(), 0);
        assert_eq!(graph.edge_count(), 0);
        assert_eq!(graph.version(), crate::engine::CURRENT_VERSION);
    }

    #[test]
    fn test_graph_serialization_roundtrip() {
        use crate::engine::builder::CodeGraphBuilder;
        use crate::model::{GraphNode, NodeKind};
        use std::sync::Arc;

        let mut builder = CodeGraphBuilder::new();
        let node = GraphNode {
            id: Arc::from("test.node"),
            name: smol_str::SmolStr::from("node"),
            kind: NodeKind::Class,
            lang: Arc::from("java"),
            location: None,
            metadata: serde_json::Value::Null,
        };
        builder.add_node(Arc::from("test.node"), node);
        let graph = builder.build();

        let serialized = graph.serialize().expect("Serialization failed");
        let deserialized = CodeGraph::deserialize(&serialized).expect("Deserialization failed");

        assert_eq!(deserialized.node_count(), 1);
        let idx = deserialized.find_node("test.node").unwrap();
        let recovered_node = &deserialized.topology()[idx];
        assert_eq!(recovered_node.name, "node");
        assert_eq!(recovered_node.lang.as_ref(), "java");
    }
}
