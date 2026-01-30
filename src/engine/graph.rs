//! Arc-wrapped immutable code graph
//!
//! The `CodeGraph` provides a cheap-to-clone, immutable view of the indexed codebase.
//! All data is wrapped in `Arc`, so cloning only increments a reference counter.

use crate::model::graph::{GraphEdge, GraphNode};
use crate::project::source::SourceFile;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Immutable code graph (cheap to clone via Arc)
#[derive(Clone)]
pub struct CodeGraph {
    inner: std::sync::Arc<CodeGraphInner>,
}

/// Internal data structure (shared via Arc)
#[derive(Serialize, Deserialize, Clone)]
pub(crate) struct CodeGraphInner {
    pub version: u32,
    pub topology: StableDiGraph<GraphNode, GraphEdge>,
    pub fqn_map: HashMap<String, NodeIndex>,
    pub name_map: HashMap<String, Vec<NodeIndex>>,
    pub file_map: HashMap<PathBuf, SourceFile>,
    pub path_to_nodes: HashMap<PathBuf, Vec<NodeIndex>>,
}

impl CodeGraph {
    /// Create an empty graph
    pub fn empty() -> Self {
        Self {
            inner: std::sync::Arc::new(CodeGraphInner {
                version: crate::engine::CURRENT_VERSION,
                topology: StableDiGraph::new(),
                fqn_map: HashMap::new(),
                name_map: HashMap::new(),
                file_map: HashMap::new(),
                path_to_nodes: HashMap::new(),
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

    /// Get reference to the FQN map
    pub fn fqn_map(&self) -> &HashMap<String, NodeIndex> {
        &self.inner.fqn_map
    }

    /// Get reference to the name map
    pub fn name_map(&self) -> &HashMap<String, Vec<NodeIndex>> {
        &self.inner.name_map
    }

    /// Get reference to the file map
    pub fn file_map(&self) -> &HashMap<PathBuf, SourceFile> {
        &self.inner.file_map
    }

    /// Get reference to the path-to-nodes map
    pub fn path_to_nodes(&self) -> &HashMap<PathBuf, Vec<NodeIndex>> {
        &self.inner.path_to_nodes
    }

    /// Find node index by FQN
    pub fn find_node(&self, fqn: &str) -> Option<NodeIndex> {
        self.inner.fqn_map.get(fqn).copied()
    }

    /// Get node data by index
    pub fn get_node(&self, idx: NodeIndex) -> Option<&GraphNode> {
        self.inner.topology.node_weight(idx)
    }

    /// Find node at a specific location in a file
    pub fn find_node_at(&self, path: &Path, line: usize, col: usize) -> Option<NodeIndex> {
        let nodes = self.inner.path_to_nodes.get(path)?;

        for &idx in nodes {
            if let Some(node) = self.inner.topology.node_weight(idx) {
                if let Some(range) = node.name_range() {
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
        if let Some(&idx) = self.inner.fqn_map.get(fqn) {
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
    pub fn serialize(&self) -> Result<Vec<u8>, rmp_serde::encode::Error> {
        rmp_serde::to_vec(&*self.inner)
    }

    /// Deserialize from bytes
    pub fn deserialize(bytes: &[u8]) -> Result<Self, rmp_serde::decode::Error> {
        let inner: CodeGraphInner = rmp_serde::from_slice(bytes)?;
        Ok(Self::from_inner(inner))
    }

    /// Save graph to JSON file (for debugging)
    pub fn save_to_json<P: AsRef<std::path::Path>>(&self, path: P) -> crate::error::Result<()> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &*self.inner)
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
}
