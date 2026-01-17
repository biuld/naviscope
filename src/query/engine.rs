use crate::error::{NaviscopeError, Result};
use crate::index::NaviscopeIndex;
use crate::model::graph::EdgeType;
use crate::query::dsl::GraphQuery;
use crate::query::model::NodeSummary;
use petgraph::Direction;
use regex::RegexBuilder;

pub struct QueryEngine<'a> {
    index: &'a NaviscopeIndex,
}

impl<'a> QueryEngine<'a> {
    pub fn new(index: &'a NaviscopeIndex) -> Self {
        Self { index }
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

                for node in self.index.graph.node_weights() {
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
                    // 缺省 FQN 时，列出所有的顶级模块 (具有文件路径的 Gradle Package 节点)
                    let mut results = Vec::new();
                    for node in self.index.graph.node_weights() {
                        if let crate::model::graph::GraphNode::Build(crate::model::graph::BuildElement::Gradle { 
                            element: crate::model::lang::gradle::GradleElement::Package(_), 
                            file_path 
                        }) = node {
                            // 只有真正的项目模块节点才会关联 build.gradle 的 file_path
                            // Java 的 package 节点虽然也用 GradleElement::Package 但没有 file_path
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
                if let Some(&idx) = self.index.fqn_map.get(fqn) {
                    let node = &self.index.graph[idx];
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
            .index
            .fqn_map
            .get(fqn)
            .ok_or_else(|| NaviscopeError::Parsing(format!("Node not found: {}", fqn)))?;

        let mut results = Vec::new();
        let mut edges = self
            .index
            .graph
            .neighbors_directed(*start_idx, dir)
            .detach();

        while let Some((edge_idx, neighbor_idx)) = edges.next(&self.index.graph) {
            let edge_data = &self.index.graph[edge_idx];
            if edge_filter.is_empty() || edge_filter.contains(edge_data) {
                let neighbor_node = &self.index.graph[neighbor_idx];
                let summary = NodeSummary::from(neighbor_node);

                if kind_filter.is_empty() || kind_filter.contains(&summary.kind) {
                    results.push(summary);
                }
            }
        }

        Ok(serde_json::to_value(results)?)
    }
}
