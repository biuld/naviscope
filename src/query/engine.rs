use crate::error::{NaviscopeError, Result};
use crate::index::CodeGraph;
use crate::model::graph::{EdgeType, NodeKind};
use crate::query::dsl::GraphQuery;
use crate::query::model::{QueryResult, QueryResultEdge};
use petgraph::Direction as PetDirection;
use regex::RegexBuilder;

pub struct QueryEngine<'a> {
    graph: &'a CodeGraph,
}

impl<'a> QueryEngine<'a> {
    pub fn new(graph: &'a CodeGraph) -> Self {
        Self { graph }
    }

    pub fn execute(&self, query: &GraphQuery) -> Result<QueryResult> {
        match query {
            GraphQuery::Grep {
                pattern,
                kind,
                limit,
            } => {
                let regex = RegexBuilder::new(pattern)
                    .case_insensitive(true)
                    .build()
                    .map_err(|e| NaviscopeError::Parsing(format!("Invalid regex: {}", e)))?;

                let mut nodes = Vec::new();

                for node in self.graph.topology.node_weights() {
                    // Check if either FQN or Name matches the pattern
                    if regex.is_match(node.fqn()) || regex.is_match(node.name()) {
                        if kind.is_empty() || kind.contains(&node.kind()) {
                            nodes.push(node.clone());
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
                    for idx in self.graph.topology.node_indices() {
                        let node = &self.graph.topology[idx];
                        if node.kind() == NodeKind::Module {
                            let has_parent = self
                                .graph
                                .topology
                                .edges_directed(idx, PetDirection::Incoming)
                                .any(|e| e.weight().edge_type == EdgeType::Contains);

                            if !has_parent {
                                nodes.push(node.clone());
                            }
                        }
                    }

                    // 2. If no top-level modules, but user asked for specific kind or we found nothing
                    if nodes.is_empty() {
                        for idx in self.graph.topology.node_indices() {
                            let node = &self.graph.topology[idx];
                            let has_parent = self
                                .graph
                                .topology
                                .edges_directed(idx, PetDirection::Incoming)
                                .any(|e| e.weight().edge_type == EdgeType::Contains);

                            if !has_parent {
                                if kind.is_empty() || kind.contains(&node.kind()) {
                                    nodes.push(node.clone());
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
                if let Some(&idx) = self.graph.fqn_map.get(fqn) {
                    let node = &self.graph.topology[idx];
                    Ok(QueryResult::new(vec![node.clone()], vec![]))
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
                self.traverse_neighbors(fqn, edge_types, direction, &[])
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
        let start_idx = self.graph.fqn_map.get(fqn).ok_or_else(|| {
            // Debug log to help identify the mismatch
            eprintln!(
                "DEBUG: traverse_neighbors failed. Looking for FQN: '{}'",
                fqn
            );
            eprintln!("DEBUG: Available FQNs count: {}", self.graph.fqn_map.len());
            if let Some(closest) = self.graph.fqn_map.keys().find(|k| k.contains(fqn)) {
                eprintln!("DEBUG: Found something containing '{}': '{}'", fqn, closest);
            }
            NaviscopeError::Parsing(format!("Node not found: {}", fqn))
        })?;

        let mut nodes = Vec::new();
        let mut edges_result = Vec::new();
        let mut edges = self
            .graph
            .topology
            .neighbors_directed(*start_idx, dir)
            .detach();

        while let Some((edge_idx, neighbor_idx)) = edges.next(&self.graph.topology) {
            let edge_data = &self.graph.topology[edge_idx];
            if edge_filter.is_empty() || edge_filter.contains(&edge_data.edge_type) {
                let neighbor_node = &self.graph.topology[neighbor_idx];
                let start_node = &self.graph.topology[*start_idx];

                if kind_filter.is_empty() || kind_filter.contains(&neighbor_node.kind()) {
                    nodes.push(neighbor_node.clone());

                    let (from, to) = if dir == PetDirection::Outgoing {
                        (
                            start_node.fqn().to_string(),
                            neighbor_node.fqn().to_string(),
                        )
                    } else {
                        (
                            neighbor_node.fqn().to_string(),
                            start_node.fqn().to_string(),
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
