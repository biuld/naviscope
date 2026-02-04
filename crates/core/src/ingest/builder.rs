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
use naviscope_plugin::{FqnInterner, ModelConverter};
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use std::collections::HashMap;
use std::path::Path;

/// Mutable graph builder
pub struct CodeGraphBuilder {
    inner: CodeGraphInner,
    pub naming_conventions:
        HashMap<crate::model::Language, std::sync::Arc<dyn naviscope_plugin::NamingConvention>>,
}

impl CodeGraphBuilder {
    /// Create a new empty builder
    pub fn new() -> Self {
        let rodeo = std::sync::Arc::new(lasso::ThreadedRodeo::new());
        Self {
            inner: CodeGraphInner {
                instance_id: 0, // Will be updated when built
                version: crate::model::graph::CURRENT_VERSION,
                topology: StableDiGraph::new(),
                fqns: crate::model::FqnManager::with_rodeo(rodeo.clone()),
                symbols: rodeo,
                fqn_index: HashMap::new(),
                name_index: HashMap::new(),
                file_index: HashMap::new(),
                reference_index: HashMap::new(),
            },
            naming_conventions: HashMap::new(),
        }
    }

    /// Create builder from existing graph (deep copy)
    pub fn from_graph(graph: &CodeGraph) -> Self {
        graph.to_builder()
    }

    /// Create builder from internal data
    pub(crate) fn from_inner(inner: CodeGraphInner) -> Self {
        Self {
            inner,
            naming_conventions: HashMap::new(),
        }
    }

    // ---- Mutation methods ----

    /// Resolve and intern an ID, potentially upgrading it using NamingConventions
    fn resolve_storage_id(
        &self,
        id: &naviscope_api::models::symbol::NodeId,
        kind_hint: Option<naviscope_api::models::NodeKind>,
    ) -> naviscope_api::models::symbol::FqnId {
        // We need to guess the language to pick the convention.
        // But NodeId doesn't carry language info directly unless we infer it or it's passed.
        // HOWEVER, `id` might be lang-specific.
        // For simplicity, we iterate over available conventions or try to detect.
        // Since we typically have one language active per builder usage OR we can't easily know,
        // we might have to rely on the fact that `Java` is likely the one needing this.
        // A better approach: The `GraphOp` or `IndexNode` context.
        // But `AddEdge` has no context.
        // Let's iterate all conventions. If any convention claims it, we use it?
        // Or simpler: Java is special.

        // BETTER: `naming_conventions` is a map. If we have keys, we try them.
        for (_, nc) in &self.naming_conventions {
            match id {
                naviscope_api::models::symbol::NodeId::Flat(s) => {
                    // Try to upgrade
                    // We don't know if this ID belongs to 'lang', but we can try parsing.
                    // A cleaner way is if the ID string itself gives a hint, but it doesn't.
                    // For now, if we have a convention, we USE it.
                    // This assumes we don't mix conflicting conventions in one builder session recklessly.
                    let parts = nc.parse_fqn(s, kind_hint.clone());
                    let structured_id = naviscope_api::models::symbol::NodeId::Structured(parts);
                    return self.inner.fqns.intern_node_id(&structured_id);
                }
                _ => {}
            }
        }

        self.inner.fqns.intern_node_id(id)
    }

    /// Add or update a node
    pub fn add_node(&mut self, node_data: crate::ingest::parser::IndexNode) -> NodeIndex {
        // We have language info here! Use it to select convention.
        let lang = crate::model::Language::new(node_data.lang.clone());
        let fqn_id = if let Some(nc) = self.naming_conventions.get(&lang) {
            match &node_data.id {
                naviscope_api::models::symbol::NodeId::Flat(s) => {
                    let parts = nc.parse_fqn(s, Some(node_data.kind.clone()));
                    let structured_id = naviscope_api::models::symbol::NodeId::Structured(parts);
                    self.inner.fqns.intern_node_id(&structured_id)
                }
                _ => self.inner.fqns.intern_node_id(&node_data.id),
            }
        } else {
            // Fallback to "any convention" or generic
            self.resolve_storage_id(&node_data.id, Some(node_data.kind.clone()))
        };

        if let Some(&idx) = self.inner.fqn_index.get(&fqn_id) {
            // Node already exists
            idx
        } else {
            let name_sym = self.inner.fqns.intern_atom(&node_data.name);
            let lang_sym = self.inner.fqns.intern_atom(&node_data.lang);
            let location = node_data
                .location
                .as_ref()
                .map(|l| l.to_internal(&self.inner.fqns));

            let mut ctx = crate::model::storage::model::GenericStorageContext {
                rodeo: self.inner.symbols.clone(),
            };

            let node = crate::model::GraphNode {
                id: fqn_id,
                name: name_sym,
                kind: node_data.kind.clone(),
                lang: lang_sym,
                location: location.clone(),
                metadata: node_data.metadata.intern(&mut ctx),
            };

            let idx = self.inner.topology.add_node(node);
            self.inner.fqn_index.insert(fqn_id, idx);
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
            let _name = node.name;

            // Remove from indices
            self.inner.fqn_index.remove(&fqn);

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
                let from_fqn = self.resolve_storage_id(&from_id, None);
                let to_fqn = self.resolve_storage_id(&to_id, None);

                match (
                    self.inner.fqn_index.get(&from_fqn),
                    self.inner.fqn_index.get(&to_fqn),
                ) {
                    (Some(&from), Some(&to)) => {
                        self.add_edge(from, to, edge);
                    }
                    _ => {
                        eprintln!(
                            "Failed to add edge: from={:?} (found={}), to={:?} (found={})",
                            from_fqn,
                            self.inner.fqn_index.contains_key(&from_fqn),
                            to_fqn,
                            self.inner.fqn_index.contains_key(&to_fqn)
                        );
                    }
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

    /// Apply multiple graph operations in a single atomic-like batch.
    /// Reorders ops to ensure correct application:
    /// 1. Removals
    /// 2. Node additions & updates
    /// 3. Edge additions (Relational)
    pub fn apply_ops(&mut self, ops: Vec<GraphOp>) -> crate::error::Result<()> {
        let mut destructive = Vec::new();
        let mut additive = Vec::new();
        let mut relational = Vec::new();

        for op in ops {
            match op {
                GraphOp::RemovePath { .. } => destructive.push(op),
                GraphOp::AddNode { .. }
                | GraphOp::UpdateFile { .. }
                | GraphOp::UpdateIdentifiers { .. } => additive.push(op),
                GraphOp::AddEdge { .. } => relational.push(op),
            }
        }

        for op in destructive {
            self.apply_op(op)?;
        }
        for op in additive {
            self.apply_op(op)?;
        }
        for op in relational {
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
            id: "test_project".into(),
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
            id: "new_project".into(),
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
