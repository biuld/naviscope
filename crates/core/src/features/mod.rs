use naviscope_api::models::symbol::Symbol;
use std::path::Path;

pub mod discovery;
pub mod navigation;
pub mod query;

/// Trait to abstract over different CodeGraph implementations for features.
/// This allows features to operate on both the full indexed graph and partial/mocked graphs for tests.
pub trait CodeGraphLike: Send + Sync {
    fn topology(
        &self,
    ) -> &petgraph::stable_graph::StableDiGraph<crate::model::GraphNode, crate::model::GraphEdge>;
    fn fqn_map(&self) -> &std::collections::HashMap<Symbol, petgraph::stable_graph::NodeIndex>;
    fn path_to_nodes(&self, path: &Path) -> Option<&[petgraph::stable_graph::NodeIndex]>;
    fn reference_index(&self) -> &std::collections::HashMap<Symbol, Vec<Symbol>>;
    fn find_container_node_at(
        &self,
        path: &std::path::Path,
        line: usize,
        col: usize,
    ) -> Option<petgraph::stable_graph::NodeIndex>;
    fn symbols(&self) -> &lasso::Rodeo;

    /// Helper to find node by string FQN
    fn find_node(&self, fqn: &str) -> Option<petgraph::stable_graph::NodeIndex> {
        let key = self.symbols().get(fqn)?;
        self.fqn_map().get(&Symbol(key)).copied()
    }

    /// Find all nodes matching an FQN string (handle duplicates if any)
    fn find_matches_by_fqn(&self, fqn: &str) -> Vec<petgraph::stable_graph::NodeIndex> {
        if let Some(idx) = self.find_node(fqn) {
            vec![idx]
        } else {
            vec![]
        }
    }
}

// Blanket implementation for references
impl<T: CodeGraphLike> CodeGraphLike for &T {
    fn topology(
        &self,
    ) -> &petgraph::stable_graph::StableDiGraph<crate::model::GraphNode, crate::model::GraphEdge>
    {
        (*self).topology()
    }

    fn fqn_map(&self) -> &std::collections::HashMap<Symbol, petgraph::stable_graph::NodeIndex> {
        (*self).fqn_map()
    }

    fn path_to_nodes(&self, path: &Path) -> Option<&[petgraph::stable_graph::NodeIndex]> {
        (*self).path_to_nodes(path)
    }

    fn reference_index(&self) -> &std::collections::HashMap<Symbol, Vec<Symbol>> {
        (*self).reference_index()
    }

    fn find_container_node_at(
        &self,
        path: &std::path::Path,
        line: usize,
        col: usize,
    ) -> Option<petgraph::stable_graph::NodeIndex> {
        (*self).find_container_node_at(path, line, col)
    }

    fn symbols(&self) -> &lasso::Rodeo {
        (*self).symbols()
    }
}

// Implement for the core CodeGraph model
impl CodeGraphLike for crate::model::CodeGraph {
    fn topology(
        &self,
    ) -> &petgraph::stable_graph::StableDiGraph<crate::model::GraphNode, crate::model::GraphEdge>
    {
        self.topology()
    }

    fn fqn_map(&self) -> &std::collections::HashMap<Symbol, petgraph::stable_graph::NodeIndex> {
        self.fqn_map()
    }

    fn path_to_nodes(&self, path: &Path) -> Option<&[petgraph::stable_graph::NodeIndex]> {
        let key = self.symbols().get(path.to_string_lossy())?;
        self.file_index()
            .get(&Symbol(key))
            .map(|e| e.nodes.as_slice())
    }

    fn reference_index(&self) -> &std::collections::HashMap<Symbol, Vec<Symbol>> {
        self.reference_index()
    }

    fn find_container_node_at(
        &self,
        path: &std::path::Path,
        line: usize,
        col: usize,
    ) -> Option<petgraph::stable_graph::NodeIndex> {
        self.find_container_node_at(path, line, col)
    }

    fn symbols(&self) -> &lasso::Rodeo {
        self.symbols()
    }
}
