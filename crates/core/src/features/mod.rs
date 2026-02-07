use crate::model::FqnManager;
use naviscope_api::models::symbol::{FqnId, Symbol};
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
    fn fqn_map(&self) -> &std::collections::HashMap<FqnId, petgraph::stable_graph::NodeIndex>;
    fn path_to_nodes(&self, path: &Path) -> Option<&[petgraph::stable_graph::NodeIndex]>;
    fn reference_index(&self) -> &std::collections::HashMap<Symbol, Vec<Symbol>>;
    fn find_container_node_at(
        &self,
        path: &std::path::Path,
        line: usize,
        col: usize,
    ) -> Option<petgraph::stable_graph::NodeIndex>;
    fn fqns(&self) -> &FqnManager;
    fn symbols(&self) -> &lasso::ThreadedRodeo;
    fn as_plugin_graph(&self) -> &dyn naviscope_plugin::CodeGraph;

    /// Helper to render a node's full FQN with optional naming convention
    fn render_fqn(
        &self,
        node: &crate::model::GraphNode,
        convention: Option<&dyn naviscope_plugin::NamingConvention>,
    ) -> String {
        use naviscope_plugin::NamingConvention;

        if let Some(nc) = convention {
            nc.render_fqn(node.id, self.fqns())
        } else {
            // Fallback to default dot convention
            naviscope_plugin::DotPathConvention.render_fqn(node.id, self.fqns())
        }
    }

    /// Helper to find node by string FQN
    fn find_node(&self, fqn: &str) -> Option<petgraph::stable_graph::NodeIndex> {
        let ids = self.fqns().resolve_fqn_string(fqn);
        for id in ids {
            if let Some(&idx) = self.fqn_map().get(&id) {
                return Some(idx);
            }
        }
        None
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

    fn fqn_map(&self) -> &std::collections::HashMap<FqnId, petgraph::stable_graph::NodeIndex> {
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

    fn symbols(&self) -> &lasso::ThreadedRodeo {
        (*self).symbols()
    }

    fn fqns(&self) -> &FqnManager {
        (*self).fqns()
    }

    fn as_plugin_graph(&self) -> &dyn naviscope_plugin::CodeGraph {
        (*self).as_plugin_graph()
    }
}
