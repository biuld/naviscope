//! Arc-wrapped immutable code graph
//!
//! The `CodeGraph` provides a cheap-to-clone, immutable view of the indexed codebase.
//! All data is wrapped in `Arc`, so cloning only increments a reference counter.
use crate::error::{NaviscopeError, Result};
use crate::ingest::builder::CodeGraphBuilder;

use crate::features::CodeGraphLike;
use crate::model::source::SourceFile;
use crate::model::{GraphEdge, GraphNode};
use crate::plugin::NodeAdapter;
use crate::model::FqnManager;
use lasso::ThreadedRodeo;
use naviscope_api::models::symbol::{FqnId, Symbol};
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);

pub const CURRENT_VERSION: u32 = 1;

fn next_instance_id() -> u64 {
    NEXT_INSTANCE_ID.fetch_add(1, Ordering::Relaxed)
}

/// Immutable code graph (cheap to clone via Arc)
#[derive(Clone)]
pub struct CodeGraph {
    inner: std::sync::Arc<CodeGraphInner>,
}

/// Internal data structure (shared via Arc)
#[derive(Clone)]
pub struct CodeGraphInner {
    /// Unique instance ID for concurrency control (not serialized)
    pub instance_id: u64,

    pub version: u32,
    pub topology: StableDiGraph<GraphNode, GraphEdge>,

    /// FQN manager: structured IDs and atoms
    pub fqns: FqnManager,

    /// Core symbol table: legacy or flat interner (for non-ID symbols like paths)
    pub symbols: Arc<ThreadedRodeo>,

    /// FQN -> NodeIndex mapping for fast lookup
    pub fqn_index: HashMap<FqnId, NodeIndex>,

    /// Simple name -> NodeIndices for symbol search
    pub name_index: HashMap<Symbol, Vec<NodeIndex>>,

    /// File-level information: metadata and nodes contained in each file
    pub file_index: HashMap<Symbol, FileEntry>,

    /// Reference Index: Token (e.g. Method Name) -> Files that contain this token.
    /// Used for fast "scouting" during reference discovery.
    pub reference_index: HashMap<Symbol, Vec<Symbol>>,
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
        let rodeo = std::sync::Arc::new(lasso::ThreadedRodeo::new());
        Self {
            inner: std::sync::Arc::new(CodeGraphInner {
                instance_id: next_instance_id(),
                version: CURRENT_VERSION,
                topology: StableDiGraph::new(),
                fqns: FqnManager::with_rodeo(rodeo.clone()),
                symbols: rodeo,
                fqn_index: HashMap::new(),
                name_index: HashMap::new(),
                file_index: HashMap::new(),
                reference_index: HashMap::new(),
            }),
        }
    }

    /// Create graph from internal data
    pub(crate) fn from_inner(mut inner: CodeGraphInner) -> Self {
        inner.instance_id = next_instance_id();
        Self {
            inner: std::sync::Arc::new(inner),
        }
    }

    /// Create a builder for modifying this graph
    ///
    /// Note: This performs a deep copy, so it should only be called when
    /// building/updating the index, not during queries.
    pub fn to_builder(&self) -> CodeGraphBuilder {
        CodeGraphBuilder::from_inner((*self.inner).clone())
    }

    // ---- Read-only accessors ----

    /// Get the unique instance ID for this graph version
    pub fn instance_id(&self) -> u64 {
        self.inner.instance_id
    }

    /// Get the version number
    pub fn version(&self) -> u32 {
        self.inner.version
    }

    pub fn symbols(&self) -> &lasso::ThreadedRodeo {
        &self.inner.symbols
    }

    pub fn fqns(&self) -> &FqnManager {
        &self.inner.fqns
    }

    /// Get reference to the topology graph
    pub fn topology(&self) -> &StableDiGraph<GraphNode, GraphEdge> {
        &self.inner.topology
    }

    /// Get reference to the FQN index
    pub fn fqn_map(&self) -> &HashMap<FqnId, NodeIndex> {
        &self.inner.fqn_index
    }

    /// Get reference to the name index
    pub fn name_map(&self) -> &HashMap<Symbol, Vec<NodeIndex>> {
        &self.inner.name_index
    }

    /// Get reference to the file index
    pub fn file_index(&self) -> &HashMap<Symbol, FileEntry> {
        &self.inner.file_index
    }

    /// Get reference to the reference index
    pub fn reference_index(&self) -> &HashMap<Symbol, Vec<Symbol>> {
        &self.inner.reference_index
    }

    /// Find node index by FQN (flat string)
    /// If multiple nodes match (e.g. overloads), it returns the first one found.
    pub fn find_node(&self, fqn: &str) -> Option<NodeIndex> {
        let ids = self.inner.fqns.resolve_fqn_string(fqn);
        for id in ids {
            if let Some(&idx) = self.inner.fqn_index.get(&id) {
                return Some(idx);
            }
        }
        None
    }

    /// Get node data by index
    pub fn get_node(&self, idx: NodeIndex) -> Option<&GraphNode> {
        self.inner.topology.node_weight(idx)
    }

    /// Find node at a specific location in a file (by name range)
    pub fn find_node_at(&self, path: &Path, line: usize, col: usize) -> Option<NodeIndex> {
        let path_str = path.to_string_lossy();
        let key = self.inner.symbols.get(path_str.as_ref())?;
        let entry = self.inner.file_index.get(&Symbol(key))?;

        for &idx in &entry.nodes {
            if let Some(node) = self.inner.topology.node_weight(idx) {
                let range_opt: Option<&naviscope_api::models::symbol::Range> = node.name_range();
                if let Some(range) = range_opt {
                    if range.contains(line, col) {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Find the smallest node whose full range contains the specific location
    pub fn find_container_node_at(
        &self,
        path: &Path,
        line: usize,
        col: usize,
    ) -> Option<NodeIndex> {
        let path_str = path.to_string_lossy();
        let key = self.inner.symbols.get(path_str.as_ref())?;
        let entry = self.inner.file_index.get(&Symbol(key))?;

        let mut best_node = None;
        let mut min_range_size = usize::MAX;

        for &idx in &entry.nodes {
            if let Some(node) = self.inner.topology.node_weight(idx) {
                if let Some(range) = node.range() {
                    if range.contains(line, col) {
                        // Calculate a rough size to find the smallest enclosing node
                        let size = (range.end_line - range.start_line) * 1000
                            + (range.end_col.saturating_sub(range.start_col));
                        if size < min_range_size {
                            min_range_size = size;
                            best_node = Some(idx);
                        }
                    }
                }
            }
        }
        best_node
    }

    /// Find all nodes matching a symbol resolution result (FQN string)
    pub fn find_matches_by_fqn(&self, fqn: &str) -> Vec<NodeIndex> {
        let ids = self.inner.fqns.resolve_fqn_string(fqn);
        let mut results = Vec::new();
        for id in ids {
            if let Some(&idx) = self.inner.fqn_index.get(&id) {
                results.push(idx);
            }
        }
        results
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
    pub fn serialize(
        &self,
        get_plugin: impl Fn(&str) -> Option<Arc<dyn NodeAdapter>>,
    ) -> Result<Vec<u8>> {
        use super::storage::to_storage;
        let storage = to_storage(&self.inner, get_plugin);
        let bytes = rmp_serde::to_vec(&storage)
            .map_err(|e| NaviscopeError::Internal(format!("MSGPACK error: {}", e)))?;

        let compressed = zstd::encode_all(&bytes[..], 0)
            .map_err(|e| NaviscopeError::Internal(format!("Zstd compression failed: {}", e)))?;

        Ok(compressed)
    }

    /// Deserialize from bytes
    pub fn deserialize(
        bytes: &[u8],
        get_plugin: impl Fn(&str) -> Option<Arc<dyn NodeAdapter>>,
    ) -> Result<Self> {
        use super::storage::{StorageGraph, from_storage};

        // Decompress using streaming decoder to save memory
        let decoder = zstd::stream::read::Decoder::new(bytes)
            .map_err(|e| NaviscopeError::Internal(format!("Zstd decoder init failed: {}", e)))?;

        let storage: StorageGraph = rmp_serde::from_read(decoder)
            .map_err(|e| NaviscopeError::Internal(format!("MSGPACK error: {}", e)))?;

        let inner = from_storage(storage, get_plugin);
        Ok(Self::from_inner(inner))
    }

    /// Save graph to JSON file (for debugging)
    pub fn save_to_json<P: AsRef<std::path::Path>>(
        &self,
        path: P,
        get_plugin: impl Fn(&str) -> Option<Arc<dyn NodeAdapter>>,
    ) -> crate::error::Result<()> {
        use super::storage::to_storage;
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        let storage = to_storage(&self.inner, get_plugin);
        serde_json::to_writer_pretty(writer, &storage)
            .map_err(|e| crate::error::NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }
}

impl CodeGraphLike for CodeGraph {
    fn topology(
        &self,
    ) -> &petgraph::stable_graph::StableDiGraph<crate::model::GraphNode, crate::model::GraphEdge>
    {
        &self.inner.topology
    }

    fn fqn_map(&self) -> &std::collections::HashMap<FqnId, petgraph::stable_graph::NodeIndex> {
        &self.inner.fqn_index
    }

    fn path_to_nodes(&self, path: &Path) -> Option<&[petgraph::stable_graph::NodeIndex]> {
        let key = self.inner.symbols.get(path.to_string_lossy())?;
        self.inner
            .file_index
            .get(&Symbol(key))
            .map(|e| e.nodes.as_slice())
    }

    fn reference_index(&self) -> &std::collections::HashMap<Symbol, Vec<Symbol>> {
        &self.inner.reference_index
    }

    fn find_container_node_at(
        &self,
        path: &std::path::Path,
        line: usize,
        col: usize,
    ) -> Option<petgraph::stable_graph::NodeIndex> {
        Self::find_container_node_at(self, path, line, col)
    }

    fn symbols(&self) -> &lasso::ThreadedRodeo {
        &self.inner.symbols
    }

    fn fqns(&self) -> &FqnManager {
        &self.inner.fqns
    }

    fn find_node(&self, fqn: &str) -> Option<petgraph::stable_graph::NodeIndex> {
        Self::find_node(self, fqn)
    }

    fn find_matches_by_fqn(&self, fqn: &str) -> Vec<petgraph::stable_graph::NodeIndex> {
        Self::find_matches_by_fqn(self, fqn)
    }
}
#[cfg(test)]
mod tests {
    use super::{CURRENT_VERSION, CodeGraph};

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
        assert_eq!(graph.version(), CURRENT_VERSION);
    }

    #[test]
    fn test_graph_serialization_roundtrip() {
        use crate::ingest::builder::CodeGraphBuilder;
        use crate::model::NodeKind;

        let mut builder = CodeGraphBuilder::new();
        let node = crate::ingest::parser::IndexNode {
            id: naviscope_api::models::symbol::NodeId::Flat("test_node".to_string()),
            name: "node".to_string(),
            kind: NodeKind::Class,
            lang: "java".to_string(),
            location: None,
            metadata: std::sync::Arc::new(crate::model::EmptyMetadata),
        };
        builder.add_node(node);
        let graph = builder.build();

        let serialized = graph.serialize(|_| None).expect("Serialization failed");
        let deserialized =
            CodeGraph::deserialize(&serialized, |_| None).expect("Deserialization failed");

        assert_eq!(deserialized.node_count(), 1);
        let idx = deserialized.find_node("test_node").unwrap();
        let recovered_node = &deserialized.topology()[idx];

        let symbols = deserialized.symbols();
        assert_eq!(recovered_node.name(symbols), "node");
        assert_eq!(recovered_node.language(symbols).as_str(), "java");
    }
}
