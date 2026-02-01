use crate::error::{NaviscopeError, Result};
use crate::model::{DisplayGraphNode, EdgeType, NodeKind};
use crate::query::model::{QueryResult, QueryResultEdge};
use naviscope_api::models::GraphQuery;
use naviscope_api::models::symbol::Symbol;
use petgraph::Direction as PetDirection;
use regex::RegexBuilder;
use std::path::Path;
use std::sync::Arc;

// Trait to abstract over different CodeGraph implementations
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

    // Helper to find node by string FQN
    fn find_node(&self, fqn: &str) -> Option<petgraph::stable_graph::NodeIndex> {
        let key = self.symbols().get(fqn)?;
        self.fqn_map().get(&Symbol(key)).copied()
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

// Implement for new CodeGraph
impl CodeGraphLike for crate::engine::CodeGraph {
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

pub struct QueryEngine<G> {
    graph: G,
}

impl<G: CodeGraphLike> QueryEngine<G> {
    pub fn new(graph: G) -> Self {
        Self { graph }
    }

    fn to_display_node(&self, node: &crate::model::GraphNode) -> DisplayGraphNode {
        let symbols = self.graph.symbols();
        node.to_display(symbols)
    }

    pub fn execute(&self, query: &GraphQuery) -> Result<QueryResult> {
        let symbols = self.graph.symbols();
        match query {
            GraphQuery::Find {
                pattern,
                kind,
                limit,
            } => {
                let regex = RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| NaviscopeError::Parsing(format!("Invalid regex: {}", e)))?;

                let mut nodes = Vec::new();

                for node in self.graph.topology().node_weights() {
                    // Check if either FQN or Name matches the pattern
                    if regex.is_match(node.fqn(symbols)) || regex.is_match(node.name(symbols)) {
                        if kind.is_empty() || kind.contains(&node.kind) {
                            nodes.push(self.to_display_node(node));
                        }
                    }

                    if nodes.len() >= *limit {
                        break;
                    }
                }
                Ok(QueryResult::new(nodes, vec![]))
            }
            GraphQuery::Ls {
                fqn,
                kind,
                modifiers: _,
            } => {
                if let Some(target_fqn) = fqn {
                    self.traverse_neighbors(
                        target_fqn,
                        &[EdgeType::Contains],
                        PetDirection::Outgoing,
                        kind,
                    )
                } else {
                    // When FQN is missing, list all top-level nodes
                    let mut nodes = Vec::new();

                    // 1. Try to find Modules first (this is what we almost always want in root)
                    for idx in self.graph.topology().node_indices() {
                        let node = &self.graph.topology()[idx];
                        if node.kind == NodeKind::Module {
                            let has_parent = self
                                .graph
                                .topology()
                                .edges_directed(idx, PetDirection::Incoming)
                                .any(|e| e.weight().edge_type == EdgeType::Contains);

                            if !has_parent {
                                nodes.push(self.to_display_node(node));
                            }
                        }
                    }

                    // 2. If no top-level modules, but user asked for specific kind or we found nothing
                    if nodes.is_empty() {
                        for idx in self.graph.topology().node_indices() {
                            let node = &self.graph.topology()[idx];
                            let has_parent = self
                                .graph
                                .topology()
                                .edges_directed(idx, PetDirection::Incoming)
                                .any(|e| e.weight().edge_type == EdgeType::Contains);

                            if !has_parent {
                                if kind.is_empty() || kind.contains(&node.kind) {
                                    nodes.push(self.to_display_node(node));
                                }
                            }
                            if nodes.len() >= 50 {
                                break;
                            }
                        }
                    }

                    Ok(QueryResult::new(nodes, vec![]))
                }
            }
            GraphQuery::Cat { fqn } => {
                if let Some(idx) = self.graph.find_node(fqn) {
                    let node = &self.graph.topology()[idx];
                    Ok(QueryResult::new(vec![self.to_display_node(node)], vec![]))
                } else {
                    Ok(QueryResult::empty())
                }
            }
            GraphQuery::Deps {
                fqn,
                rev,
                edge_types,
            } => {
                let direction = if *rev {
                    PetDirection::Incoming
                } else {
                    PetDirection::Outgoing
                };
                self.traverse_neighbors(fqn.as_str(), edge_types, direction, &[])
            }
        }
    }

    fn traverse_neighbors(
        &self,
        fqn: &str,
        edge_filter: &[EdgeType],
        dir: PetDirection,
        kind_filter: &[NodeKind],
    ) -> Result<QueryResult> {
        let start_idx = self
            .graph
            .find_node(fqn)
            .ok_or_else(|| NaviscopeError::Parsing(format!("Node not found: {}", fqn)))?;

        let mut nodes = Vec::new();
        let mut edges_result = Vec::new();
        let topology = self.graph.topology();
        let mut edges = topology.neighbors_directed(start_idx, dir).detach();
        let symbols = self.graph.symbols();

        while let Some((edge_idx, neighbor_idx)) = edges.next(topology) {
            let edge_data = &topology[edge_idx];
            if edge_filter.is_empty() || edge_filter.contains(&edge_data.edge_type) {
                let neighbor_node = &topology[neighbor_idx];
                let start_node = &topology[start_idx];

                if kind_filter.is_empty() || kind_filter.contains(&neighbor_node.kind) {
                    nodes.push(self.to_display_node(neighbor_node));

                    let (from, to) = if dir == PetDirection::Outgoing {
                        (
                            Arc::from(start_node.fqn(symbols)),
                            Arc::from(neighbor_node.fqn(symbols)),
                        )
                    } else {
                        (
                            Arc::from(neighbor_node.fqn(symbols)),
                            Arc::from(start_node.fqn(symbols)),
                        )
                    };

                    edges_result.push(QueryResultEdge {
                        from,
                        to,
                        data: edge_data.clone(),
                    });
                }
            }
        }

        Ok(QueryResult::new(nodes, edges_result))
    }
}
