use super::CodeGraphLike;
use crate::model::{EdgeType, NodeKind};
use naviscope_api::navigation::ResolveResult;

/// NavigationEngine provides logic for resolving fuzzy/relative paths within a graph.
pub struct NavigationEngine<'a> {
    graph: &'a dyn CodeGraphLike,
}

impl<'a> NavigationEngine<'a> {
    pub fn new(graph: &'a dyn CodeGraphLike) -> Self {
        Self { graph }
    }

    pub fn resolve_path(&self, target: &str, current_context: Option<&str>) -> ResolveResult {
        // 1. Handle special paths ("/" or "root")
        if target == "/" || target == "root" {
            let project_nodes: Vec<_> = self
                .graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &self.graph.topology()[idx];
                    if matches!(node.kind(), NodeKind::Project) {
                        Some(node.fqn(self.graph.symbols()).to_string())
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
                if let Some(idx) = self.graph.find_node(current_fqn) {
                    let mut incoming = self
                        .graph
                        .topology()
                        .neighbors_directed(idx, petgraph::Direction::Incoming)
                        .detach();

                    while let Some(edge_idx) = incoming.next_edge(self.graph.topology()) {
                        let edge = &self.graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            let (parent_idx, _) =
                                self.graph.topology().edge_endpoints(edge_idx).unwrap();
                            if let Some(parent_node) = self.graph.topology().node_weight(parent_idx)
                            {
                                return ResolveResult::Found(
                                    parent_node.fqn(self.graph.symbols()).to_string(),
                                );
                            }
                        }
                    }
                }
            }
            return ResolveResult::NotFound;
        }

        // 3. Try exact match (absolute FQN)
        if self.graph.find_node(target).is_some() {
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
            if self.graph.find_node(&joined).is_some() {
                return ResolveResult::Found(joined);
            }
        }

        // 5. Try fuzzy matching (child lookup)
        let current_idx = current_context.and_then(|fqn| self.graph.find_node(fqn));

        let candidates: Vec<String> = if let Some(parent_idx) = current_idx {
            // Search in children of current node
            self.graph
                .topology()
                .neighbors_directed(parent_idx, petgraph::Direction::Outgoing)
                .filter_map(|child_idx| {
                    if let Some(edge_idx) = self.graph.topology().find_edge(parent_idx, child_idx) {
                        let edge = &self.graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            let node = &self.graph.topology()[child_idx];
                            let fqn = node.fqn(self.graph.symbols());

                            // Match by simple name (last component) or display name
                            let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                            let display_name = node.name(self.graph.symbols());
                            if simple_name == target || display_name == target {
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
            self.graph
                .fqn_map()
                .keys()
                .filter_map(|sym| {
                    let fqn = self.graph.symbols().resolve(&sym.0);
                    let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                    if simple_name == target {
                        return Some(fqn.to_string());
                    }

                    // Also check display name
                    if let Some(idx) = self.graph.fqn_map().get(sym) {
                        let node = &self.graph.topology()[*idx];
                        if node.name(self.graph.symbols()) == target {
                            return Some(fqn.to_string());
                        }
                    }

                    None
                })
                .collect()
        };

        match candidates.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(candidates[0].clone()),
            _ => ResolveResult::Ambiguous(candidates),
        }
    }

    pub fn get_completion_candidates(&self, prefix: &str) -> Vec<String> {
        self.graph
            .fqn_map()
            .keys()
            .filter_map(|sym| {
                let fqn = self.graph.symbols().resolve(&sym.0);
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
