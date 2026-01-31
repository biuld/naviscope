use super::EngineHandle;
use crate::model::{EdgeType, NodeKind};
use async_trait::async_trait;
use naviscope_api::navigation::{NavigationService, ResolveResult};

#[async_trait]
impl NavigationService for EngineHandle {
    async fn resolve_path(&self, target: &str, current_context: Option<&str>) -> ResolveResult {
        // 1. Handle special paths ("/" or "root")
        if target == "/" || target == "root" {
            let graph = self.graph().await;

            let project_nodes: Vec<_> = graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &graph.topology()[idx];
                    if matches!(node.kind(), NodeKind::Project) {
                        Some(node.fqn().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            return match project_nodes.len() {
                1 => ResolveResult::Found(project_nodes[0].clone()),
                0 => ResolveResult::Found("".to_string()),
                _ => ResolveResult::Ambiguous(project_nodes),
            };
        }

        let graph = self.graph().await;

        // 2. Handle parent navigation ("..")
        if target == ".." {
            if let Some(current_fqn) = current_context {
                if let Some(&idx) = graph.fqn_map().get(current_fqn) {
                    let mut incoming = graph
                        .topology()
                        .neighbors_directed(idx, petgraph::Direction::Incoming)
                        .detach();

                    while let Some((edge_idx, neighbor_idx)) = incoming.next(graph.topology()) {
                        let edge = &graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            if let Some(parent_node) = graph.topology().node_weight(neighbor_idx) {
                                return ResolveResult::Found(parent_node.fqn().to_string());
                            }
                        }
                    }
                }
            }
            return ResolveResult::NotFound;
        }

        // 3. Try exact match (absolute FQN)
        if graph.fqn_map().contains_key(target) {
            return ResolveResult::Found(target.to_string());
        }

        // 4. Try relative path from current context
        if let Some(current_fqn) = current_context {
            let separator = if current_fqn.contains("::") {
                "::"
            } else {
                "."
            };
            let joined = format!("{}{}{}", current_fqn, separator, target);
            if graph.fqn_map().contains_key(joined.as_str()) {
                return ResolveResult::Found(joined);
            }
        }

        // 5. Try fuzzy matching (child lookup)
        let current_idx = current_context
            .and_then(|fqn| graph.fqn_map().get(fqn))
            .copied();

        let candidates: Vec<String> = if let Some(parent_idx) = current_idx {
            // Search in children of current node
            graph
                .topology()
                .neighbors_directed(parent_idx, petgraph::Direction::Outgoing)
                .filter_map(|child_idx| {
                    // Check if edge is "Contains"
                    let edge_idx = graph.topology().find_edge(parent_idx, child_idx).unwrap();
                    let edge = &graph.topology()[edge_idx];

                    if edge.edge_type == EdgeType::Contains {
                        let node = &graph.topology()[child_idx];
                        let fqn = node.fqn();

                        // Match by simple name (last component)
                        let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                        if simple_name == target {
                            Some(fqn.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            // Global fuzzy search
            graph
                .fqn_map()
                .keys()
                .filter(|fqn| {
                    let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                    simple_name == target
                })
                .map(|s| s.to_string())
                .collect()
        };

        match candidates.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(candidates[0].clone()),
            _ => ResolveResult::Ambiguous(candidates),
        }
    }

    async fn get_completion_candidates(&self, prefix: &str) -> Vec<String> {
        let graph = self.graph().await;
        graph
            .fqn_map()
            .keys()
            .filter(|fqn| fqn.starts_with(prefix))
            .take(50) // Reasonable limit for candidates
            .map(|s| s.to_string())
            .collect()
    }
}
