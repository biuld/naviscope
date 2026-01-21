use crate::error::{NaviscopeError, Result};
use crate::index::CodeGraph;
use crate::model::graph::EdgeType;
use crate::query::dsl::GraphQuery;
use crate::query::model::NodeSummary;
use petgraph::Direction;
use regex::RegexBuilder;

pub struct QueryEngine<'a> {
    graph: &'a CodeGraph,
}

impl<'a> QueryEngine<'a> {
    pub fn new(graph: &'a CodeGraph) -> Self {
        Self { graph }
    }

    pub fn execute(&self, query: &GraphQuery) -> Result<serde_json::Value> {
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

                let mut results = Vec::new();

                for node in self.graph.topology.node_weights() {
                    let summary = NodeSummary::from(node);

                    // Check if either FQN or Name matches the pattern
                    if regex.is_match(&summary.fqn) || regex.is_match(&summary.name) {
                        if kind.is_empty() || kind.contains(&summary.kind) {
                            results.push(summary);
                        }
                    }

                    if results.len() >= *limit {
                        break;
                    }
                }
                Ok(serde_json::to_value(results)?)
            }
            GraphQuery::Ls { fqn, kind } => {
                if let Some(target_fqn) = fqn {
                    self.traverse_neighbors(target_fqn, &[EdgeType::Contains], Direction::Outgoing, kind)
                } else {
                    // When FQN is missing, list all top-level modules (Gradle Package nodes with file paths)
                    let mut results = Vec::new();
                    for node in self.graph.topology.node_weights() {
                        if let crate::model::graph::GraphNode::Build(crate::model::graph::BuildElement::Gradle { 
                            element: crate::model::lang::gradle::GradleElement::Package(_), 
                            file_path 
                        }) = node {
                            // Only actual project module nodes are associated with the file_path of build.gradle
                            // Java package nodes also use GradleElement::Package but do not have a file_path
                            if file_path.is_some() {
                                let summary = NodeSummary::from(node);
                                if kind.is_empty() || kind.contains(&summary.kind) {
                                    results.push(summary);
                                }
                            }
                        }
                    }
                    Ok(serde_json::to_value(results)?)
                }
            }
            GraphQuery::Inspect { fqn } => {
                if let Some(&idx) = self.graph.fqn_map.get(fqn) {
                    let node = &self.graph.topology[idx];
                    Ok(serde_json::to_value(node)?)
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            GraphQuery::Incoming { fqn, edge_type } => {
                self.traverse_neighbors(fqn, edge_type, Direction::Incoming, &[])
            }
            GraphQuery::Outgoing { fqn, edge_type } => {
                self.traverse_neighbors(fqn, edge_type, Direction::Outgoing, &[])
            }
        }
    }

    fn traverse_neighbors(
        &self,
        fqn: &str,
        edge_filter: &[EdgeType],
        dir: Direction,
        kind_filter: &[String],
    ) -> Result<serde_json::Value> {
        let start_idx = self
            .graph
            .fqn_map
            .get(fqn)
            .ok_or_else(|| NaviscopeError::Parsing(format!("Node not found: {}", fqn)))?;

        let mut results = Vec::new();
        let mut edges = self
            .graph
            .topology
            .neighbors_directed(*start_idx, dir)
            .detach();

        while let Some((edge_idx, neighbor_idx)) = edges.next(&self.graph.topology) {
            let edge_data = &self.graph.topology[edge_idx];
            if edge_filter.is_empty() || edge_filter.contains(&edge_data.edge_type) {
                let neighbor_node = &self.graph.topology[neighbor_idx];
                let summary = NodeSummary::from(neighbor_node);

                if kind_filter.is_empty() || kind_filter.contains(&summary.kind) {
                    results.push(summary);
                }
            }
        }

        Ok(serde_json::to_value(results)?)
    }
}
