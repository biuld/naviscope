use crate::error::{NaviscopeError, Result};
use crate::model::source::Language;
use crate::model::{DisplayGraphNode, EdgeType, NodeKind};
use crate::runtime::plugin::NodeRenderer;
pub use naviscope_api::models::{GraphQuery, QueryResult, QueryResultEdge};
use petgraph::Direction as PetDirection;
use regex::RegexBuilder;
use std::sync::Arc;

use super::CodeGraphLike;

pub struct QueryEngine<G, L> {
    graph: G,
    lookup: L,
}

impl<G, L> QueryEngine<G, L>
where
    G: CodeGraphLike,
    L: Fn(Language) -> Option<Arc<dyn NodeRenderer>>,
{
    pub fn new(graph: G, lookup: L) -> Self {
        Self { graph, lookup }
    }

    fn render_node(&self, node: &crate::model::GraphNode) -> DisplayGraphNode {
        let symbols = self.graph.symbols();
        let lang = node.language(symbols);
        if let Some(renderer) = (self.lookup)(lang.clone()) {
            renderer.render_display_node(node, symbols)
        } else {
            panic!(
                "CRITICAL: No renderer found for language '{}'. This indicates a missing plugin for indexed data.",
                lang.as_str()
            );
        }
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
                    if regex.is_match(node.fqn(symbols)) || regex.is_match(node.name(symbols)) {
                        if kind.is_empty() || kind.contains(&node.kind) {
                            nodes.push(self.render_node(node));
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
                    let mut nodes = Vec::new();

                    for idx in self.graph.topology().node_indices() {
                        let node = &self.graph.topology()[idx];
                        if node.kind == NodeKind::Module {
                            let has_parent = self
                                .graph
                                .topology()
                                .edges_directed(idx, PetDirection::Incoming)
                                .any(|e| e.weight().edge_type == EdgeType::Contains);

                            if !has_parent {
                                nodes.push(self.render_node(node));
                            }
                        }
                    }

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
                                    nodes.push(self.render_node(node));
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
                    Ok(QueryResult::new(vec![self.render_node(node)], vec![]))
                } else {
                    Ok(QueryResult::default())
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
                    nodes.push(self.render_node(neighbor_node));

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
