//! Graph builder for creating and modifying code graphs
//!
//! The `CodeGraphBuilder` allows mutable operations on the graph structure.
//! It's designed to be used during index construction/updates, then converted
//! to an immutable `CodeGraph` via the `build()` method.

use crate::model::CodeGraph;
use crate::model::graph::CodeGraphInner;
use crate::model::source::SourceFile;
// StorageContext unused
use crate::model::{GraphEdge, GraphOp};
use naviscope_api::models::symbol::Symbol;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use std::collections::HashMap;
use std::path::Path;

/// Mutable graph builder
pub struct CodeGraphBuilder {
    inner: CodeGraphInner,
}

impl CodeGraphBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        Self {
            inner: CodeGraphInner {
                instance_id: 0, // Will be updated when built
                version: crate::model::graph::CURRENT_VERSION,
                topology: StableDiGraph::new(),
                symbols: lasso::Rodeo::default(),
                fqn_index: HashMap::new(),
                name_index: HashMap::new(),
                file_index: HashMap::new(),
                reference_index: HashMap::new(),
            },
        }
    }

    /// Create builder from existing graph (deep copy)
    pub fn from_graph(graph: &CodeGraph) -> Self {
        graph.to_builder()
    }

    /// Create builder from internal data
    pub(crate) fn from_inner(inner: CodeGraphInner) -> Self {
        Self { inner }
    }

    // ---- Mutation methods ----

    /// Add or update a node
    pub fn add_node(&mut self, node_data: crate::ingest::parser::IndexNode) -> NodeIndex {
        let fqn_sym = Symbol(self.inner.symbols.get_or_intern(&node_data.id));

        if let Some(&idx) = self.inner.fqn_index.get(&fqn_sym) {
            // Node already exists
            idx
        } else {
            let name_sym = Symbol(self.inner.symbols.get_or_intern(&node_data.name));
            let lang_sym = Symbol(self.inner.symbols.get_or_intern(&node_data.lang));
            let location = node_data
                .location
                .as_ref()
                .map(|l| l.to_internal(&mut self.inner.symbols));

            let mut ctx = crate::model::storage::model::GenericStorageContext {
                rodeo: &mut self.inner.symbols,
            };

            let node = crate::model::GraphNode {
                id: fqn_sym,
                name: name_sym,
                kind: node_data.kind.clone(),
                lang: lang_sym,
                location: location.clone(),
                metadata: node_data.metadata.intern(&mut ctx),
            };

            let idx = self.inner.topology.add_node(node);
            self.inner.fqn_index.insert(fqn_sym, idx);
            self.inner.name_index.entry(name_sym).or_default().push(idx);

            if let Some(loc) = location {
                self.inner
                    .file_index
                    .entry(loc.path)
                    .and_modify(|e: &mut crate::model::graph::FileEntry| e.nodes.push(idx))
                    .or_insert_with(|| {
                        let resolved_path = self.inner.symbols.resolve(&loc.path.0);
                        crate::model::graph::FileEntry {
                            metadata: SourceFile::new(
                                std::path::PathBuf::from(resolved_path),
                                0,
                                0,
                            ),
                            nodes: vec![idx],
                        }
                    });
            }

            idx
        }
    }

    /// Add an edge between two nodes
    pub fn add_edge(&mut self, from: NodeIndex, to: NodeIndex, edge: GraphEdge) {
        // Check for duplicate edges
        let already_exists = self.inner.topology.edges_connecting(from, to).any(
            |e: petgraph::stable_graph::EdgeReference<crate::model::GraphEdge>| {
                e.weight().edge_type == edge.edge_type
            },
        );

        if !already_exists {
            self.inner.topology.add_edge(from, to, edge);
        }
    }

    /// Remove a node
    pub fn remove_node(&mut self, idx: NodeIndex) {
        if let Some(node) = self.inner.topology.node_weight(idx) {
            let fqn = node.id; // Symbol implements Copy
            let name = node.name;

            // Remove from indices
            self.inner.fqn_index.remove(&fqn);

            if let Some(nodes) = self.inner.name_index.get_mut(&name) {
                nodes.retain(|&i| i != idx);
                if nodes.is_empty() {
                    self.inner.name_index.remove(&name);
                }
            }

            // Remove from topology
            self.inner.topology.remove_node(idx);
        }
    }

    /// Remove all nodes associated with a file path
    pub fn remove_path(&mut self, path: &Path) {
        let interned_path = Symbol(
            self.inner
                .symbols
                .get_or_intern(path.to_string_lossy().as_ref()),
        );
        if let Some(entry) = self.inner.file_index.remove(&interned_path) {
            for idx in entry.nodes {
                self.remove_node(idx);
            }
        }

        // Also remove from reference_index
        for files in self.inner.reference_index.values_mut() {
            files.retain(|p| *p != interned_path);
        }
    }

    /// Update file metadata (creates or updates FileEntry)
    pub fn update_file(&mut self, path: &Path, source: SourceFile) {
        let interned_path = Symbol(
            self.inner
                .symbols
                .get_or_intern(path.to_string_lossy().as_ref()),
        );
        self.inner
            .file_index
            .entry(interned_path)
            .and_modify(|e| e.metadata = source.clone())
            .or_insert(crate::model::graph::FileEntry {
                metadata: source,
                nodes: Vec::new(),
            });
    }

    /// Apply a graph operation
    pub fn apply_op(&mut self, op: GraphOp) -> crate::error::Result<()> {
        match op {
            GraphOp::AddNode { data } => {
                if let Some(index_node) = data {
                    self.add_node(index_node);
                }
            }
            GraphOp::AddEdge {
                from_id,
                to_id,
                edge,
            } => {
                let from_sym = Symbol(self.inner.symbols.get_or_intern(from_id.as_ref()));
                let to_sym = Symbol(self.inner.symbols.get_or_intern(to_id.as_ref()));

                if let (Some(&from), Some(&to)) = (
                    self.inner.fqn_index.get(&from_sym),
                    self.inner.fqn_index.get(&to_sym),
                ) {
                    self.add_edge(from, to, edge);
                }
            }
            GraphOp::RemovePath { path } => {
                self.remove_path(&path);
            }
            GraphOp::UpdateIdentifiers { path, identifiers } => {
                let path_sym = Symbol(
                    self.inner
                        .symbols
                        .get_or_intern(path.to_string_lossy().as_ref()),
                );
                for token in identifiers {
                    let token_sym = Symbol(self.inner.symbols.get_or_intern(token.as_str()));
                    let files = self.inner.reference_index.entry(token_sym).or_default();
                    if !files.contains(&path_sym) {
                        files.push(path_sym);
                    }
                }
            }
            GraphOp::UpdateFile { metadata } => {
                let path = metadata.path.clone();
                self.update_file(&path, metadata);
            }
        }
        Ok(())
    }

    /// Apply multiple graph operations
    pub fn apply_ops(&mut self, ops: Vec<GraphOp>) -> crate::error::Result<()> {
        for op in ops {
            self.apply_op(op)?;
        }
        Ok(())
    }

    /// Build the immutable graph
    pub fn build(self) -> CodeGraph {
        CodeGraph::from_inner(self.inner)
    }
}

impl Default for CodeGraphBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::NodeKind;

    #[test]
    fn test_build_from_scratch() {
        let mut builder = CodeGraphBuilder::new();

        let node = crate::ingest::parser::IndexNode {
            id: "test_project".to_string(),
            name: "test_project".to_string(),
            kind: NodeKind::Project,
            lang: "buildfile".to_string(),
            location: None,
            metadata: std::sync::Arc::new(crate::model::EmptyMetadata),
        };

        let _idx = builder.add_node(node);
        let graph = builder.build();

        assert_eq!(graph.node_count(), 1);
        assert!(graph.find_node("test_project").is_some());
    }

    #[test]
    fn test_incremental_update() {
        let graph = CodeGraph::empty();
        assert_eq!(graph.node_count(), 0);

        let mut builder = CodeGraphBuilder::from_graph(&graph);

        let node = crate::ingest::parser::IndexNode {
            id: "new_project".to_string(),
            name: "new_project".to_string(),
            kind: NodeKind::Project,
            lang: "buildfile".to_string(),
            location: None,
            metadata: std::sync::Arc::new(crate::model::EmptyMetadata),
        };

        builder.add_node(node);
        let updated = builder.build();

        assert_eq!(updated.node_count(), 1);
    }
}
