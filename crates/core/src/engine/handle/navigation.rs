use super::EngineHandle;
use crate::model::{EdgeType, NodeKind};
use async_trait::async_trait;
use naviscope_api::navigation::{NavigationService, ResolveResult};

#[async_trait]
impl NavigationService for EngineHandle {
    async fn resolve_path(&self, target: &str, current_context: Option<&str>) -> ResolveResult {
        let graph = self.graph().await;
        // 1. Handle special paths ("/" or "root")
        if target == "/" || target == "root" {
            let project_nodes: Vec<_> = graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &graph.topology()[idx];
                    if matches!(node.kind(), NodeKind::Project) {
                        Some(node.fqn(graph.symbols()).to_string())
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

        // 2. Handle parent navigation ("..")
        if target == ".." {
            if let Some(current_fqn) = current_context {
                if let Some(idx) = graph.find_node(current_fqn) {
                    let mut incoming = graph
                        .topology()
                        .neighbors_directed(idx, petgraph::Direction::Incoming)
                        .detach();

                    while let Some(edge_idx) = incoming.next_edge(graph.topology()) {
                        let edge = &graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            let (parent_idx, _) =
                                graph.topology().edge_endpoints(edge_idx).unwrap();
                            if let Some(parent_node) = graph.topology().node_weight(parent_idx) {
                                return ResolveResult::Found(
                                    parent_node.fqn(graph.symbols()).to_string(),
                                );
                            }
                        }
                    }
                }
            }
            return ResolveResult::NotFound;
        }

        // 3. Try exact match (absolute FQN)
        if graph.find_node(target).is_some() {
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
            if graph.find_node(&joined).is_some() {
                return ResolveResult::Found(joined);
            }
        }

        // 5. Try fuzzy matching (child lookup)
        let current_idx = current_context.and_then(|fqn| graph.find_node(fqn));

        let candidates: Vec<String> = if let Some(parent_idx) = current_idx {
            // Search in children of current node
            graph
                .topology()
                .neighbors_directed(parent_idx, petgraph::Direction::Outgoing)
                .filter_map(|child_idx| {
                    // Check if edge is "Contains"
                    // Helper to find edge: stable_graph doesn't have find_edge(a,b) directly returning Index?
                    // Actually it does: find_edge(a, b) -> Option<EdgeIndex>
                    if let Some(edge_idx) = graph.topology().find_edge(parent_idx, child_idx) {
                        let edge = &graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            let node = &graph.topology()[child_idx];
                            let fqn = node.fqn(graph.symbols());

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
                .filter_map(|sym| {
                    let fqn = graph.symbols().resolve(&sym.0);
                    let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                    if simple_name == target {
                        Some(fqn.to_string())
                    } else {
                        None
                    }
                })
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
            .filter_map(|sym| {
                let fqn = graph.symbols().resolve(&sym.0);
                if fqn.starts_with(prefix) {
                    Some(fqn.to_string())
                } else {
                    None
                }
            })
            .take(50) // Reasonable limit for candidates
            .collect()
    }
}
