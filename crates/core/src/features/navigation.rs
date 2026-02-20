use super::CodeGraphLike;
use crate::model::{EdgeType, NodeKind};
use naviscope_api::navigation::ResolveResult;
use naviscope_api::{ApiError, ApiResult};

/// NavigationEngine provides logic for resolving fuzzy/relative paths within a graph.
pub struct NavigationEngine<'a> {
    graph: &'a dyn CodeGraphLike,
    naming_conventions:
        std::collections::HashMap<String, std::sync::Arc<dyn naviscope_plugin::NamingConvention>>,
}

impl<'a> NavigationEngine<'a> {
    pub fn new(
        graph: &'a dyn CodeGraphLike,
        naming_conventions: std::collections::HashMap<
            String,
            std::sync::Arc<dyn naviscope_plugin::NamingConvention>,
        >,
    ) -> Self {
        Self {
            graph,
            naming_conventions,
        }
    }

    fn get_convention(
        &self,
        node: &crate::model::GraphNode,
    ) -> Option<&dyn naviscope_plugin::NamingConvention> {
        let lang_str = self.graph.symbols().resolve(&node.lang.0);
        self.naming_conventions.get(lang_str).map(|c| c.as_ref())
    }

    pub fn resolve_path(
        &self,
        target: &str,
        current_context: Option<&str>,
    ) -> ApiResult<ResolveResult> {
        // 1. Handle special paths ("/" or "root")
        if target == "/" || target == "root" {
            let project_nodes: Vec<_> = self
                .graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &self.graph.topology()[idx];
                    if matches!(node.kind(), NodeKind::Project) {
                        let convention = self.get_convention(node);
                        Some(self.graph.render_fqn(node, convention))
                    } else {
                        None
                    }
                })
                .collect();

            return match project_nodes.len() {
                1 => Ok(ResolveResult::Found(project_nodes[0].clone())),
                0 => Err(ApiError::NotFound("project root node".to_string())),
                _ => Ok(ResolveResult::Ambiguous(project_nodes)),
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
                                let convention = self.get_convention(parent_node);
                                return Ok(ResolveResult::Found(
                                    self.graph.render_fqn(parent_node, convention),
                                ));
                            }
                        }
                    }
                }
            }
            return Ok(ResolveResult::NotFound);
        }

        // 3. Try exact match (absolute FQN)
        if self.graph.find_node(target).is_some() {
            return Ok(ResolveResult::Found(target.to_string()));
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
                return Ok(ResolveResult::Found(joined));
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
                            let convention = self.get_convention(node);
                            let fqn = self.graph.render_fqn(node, convention);

                            // Match by simple name (last component) or display name
                            let simple_name = fqn.split(&['.', ':', '#']).last().unwrap_or(&fqn);
                            let display_name = node.name(self.graph.symbols());
                            if simple_name == target || display_name == target {
                                Some(fqn)
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
                .filter_map(|&fid| {
                    if let Some(&idx) = self.graph.fqn_map().get(&fid) {
                        let node = &self.graph.topology()[idx];
                        let convention = self.get_convention(node);
                        let fqn = self.graph.render_fqn(node, convention);
                        let simple_name = fqn.split(&['.', ':', '#']).last().unwrap_or(&fqn);
                        if simple_name == target {
                            return Some(fqn);
                        }

                        // Also check display name
                        if node.name(self.graph.symbols()) == target {
                            return Some(fqn);
                        }
                    }

                    None
                })
                .collect()
        };

        Ok(match candidates.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(candidates[0].clone()),
            _ => ResolveResult::Ambiguous(candidates),
        })
    }

    pub fn get_completion_candidates(&self, prefix: &str, limit: usize) -> ApiResult<Vec<String>> {
        let candidates = self
            .graph
            .fqn_map()
            .keys()
            .filter_map(|&fid| {
                if let Some(&idx) = self.graph.fqn_map().get(&fid) {
                    let node = &self.graph.topology()[idx];
                    let convention = self.get_convention(node);
                    let fqn = self.graph.render_fqn(node, convention);
                    if fqn.starts_with(prefix) {
                        return Some(fqn);
                    }
                }
                None
            })
            .take(limit)
            .collect();
        Ok(candidates)
    }
}
