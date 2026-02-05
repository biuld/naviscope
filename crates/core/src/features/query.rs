use crate::error::{NaviscopeError, Result};
use crate::model::source::Language;
use crate::model::{DisplayGraphNode, EdgeType, NodeKind};
use crate::plugin::NodeAdapter;
pub use naviscope_api::models::{GraphQuery, QueryResult, QueryResultEdge};
use petgraph::Direction as PetDirection;
use regex::RegexBuilder;
use std::sync::Arc;

use super::CodeGraphLike;

pub struct QueryEngine<G, L> {
    graph: G,
    lookup: L,
    naming_conventions:
        std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>,
}

impl<G, L> QueryEngine<G, L>
where
    G: CodeGraphLike,
    L: Fn(Language) -> Option<Arc<dyn NodeAdapter>>,
{
    pub fn new(
        graph: G,
        lookup: L,
        naming_conventions: std::collections::HashMap<
            String,
            Arc<dyn naviscope_plugin::NamingConvention>,
        >,
    ) -> Self {
        Self {
            graph,
            lookup,
            naming_conventions,
        }
    }

    fn render_node(&self, node: &crate::model::GraphNode) -> DisplayGraphNode {
        let symbols = self.graph.symbols();
        let lang = node.language(symbols);
        if let Some(renderer) = (self.lookup)(lang.clone()) {
            renderer.render_display_node(node, self.graph.fqns())
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
                sources,
                limit,
            } => {
                let regex = RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| NaviscopeError::Parsing(format!("Invalid regex: {}", e)))?;

                let mut nodes = Vec::new();

                for node in self.graph.topology().node_weights() {
                    let lang_str = symbols.resolve(&node.lang.0);
                    let convention = self.naming_conventions.get(lang_str).map(|c| c.as_ref());
                    let fqn_str = self.graph.render_fqn(node, convention);
                    if regex.is_match(&fqn_str) || regex.is_match(node.name(symbols)) {
                        let kind_match = kind.is_empty() || kind.contains(&node.kind);
                        let source_match = sources.is_empty() || sources.contains(&node.source);
                        if kind_match && source_match {
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
                sources,
                modifiers: _,
            } => {
                if let Some(target_fqn) = fqn {
                    self.traverse_neighbors(
                        target_fqn,
                        &[EdgeType::Contains],
                        PetDirection::Outgoing,
                        kind,
                        sources,
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
                                let source_match =
                                    sources.is_empty() || sources.contains(&node.source);
                                if source_match {
                                    nodes.push(self.render_node(node));
                                }
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
                                let kind_match = kind.is_empty() || kind.contains(&node.kind);
                                let source_match =
                                    sources.is_empty() || sources.contains(&node.source);
                                if kind_match && source_match {
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
                self.traverse_neighbors(fqn.as_str(), edge_types, direction, &[], &[])
            }
        }
    }

    fn traverse_neighbors(
        &self,
        fqn: &str,
        edge_filter: &[EdgeType],
        dir: PetDirection,
        kind_filter: &[NodeKind],
        source_filter: &[naviscope_api::models::graph::NodeSource],
    ) -> Result<QueryResult> {
        let start_idx = self
            .graph
            .find_node(fqn)
            .ok_or_else(|| NaviscopeError::Parsing(format!("Node not found: {}", fqn)))?;

        let mut nodes = Vec::new();
        let mut edges_result = Vec::new();
        let topology = self.graph.topology();
        let mut edges = topology.neighbors_directed(start_idx, dir).detach();

        while let Some((edge_idx, neighbor_idx)) = edges.next(topology) {
            let edge_data = &topology[edge_idx];
            if edge_filter.is_empty() || edge_filter.contains(&edge_data.edge_type) {
                let neighbor_node = &topology[neighbor_idx];
                let start_node = &topology[start_idx];

                if (kind_filter.is_empty() || kind_filter.contains(&neighbor_node.kind))
                    && (source_filter.is_empty() || source_filter.contains(&neighbor_node.source))
                {
                    nodes.push(self.render_node(neighbor_node));

                    let symbols = self.graph.symbols();
                    let start_lang = symbols.resolve(&start_node.lang.0);
                    let neighbor_lang = symbols.resolve(&neighbor_node.lang.0);
                    let start_convention =
                        self.naming_conventions.get(start_lang).map(|c| c.as_ref());
                    let neighbor_convention = self
                        .naming_conventions
                        .get(neighbor_lang)
                        .map(|c| c.as_ref());

                    let (from, to) = if dir == PetDirection::Outgoing {
                        (
                            Arc::from(self.graph.render_fqn(start_node, start_convention)),
                            Arc::from(self.graph.render_fqn(neighbor_node, neighbor_convention)),
                        )
                    } else {
                        (
                            Arc::from(self.graph.render_fqn(neighbor_node, neighbor_convention)),
                            Arc::from(self.graph.render_fqn(start_node, start_convention)),
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
