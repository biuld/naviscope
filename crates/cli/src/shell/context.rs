use naviscope_core::engine::{CodeGraph, EngineHandle, LanguageService};
use naviscope_core::model::graph::GraphNode;
use naviscope_core::plugin::LanguageFeatureProvider;
use naviscope_core::project::source::Language;
use naviscope_core::query::GraphQuery;
use std::sync::{Arc, RwLock};

#[derive(Clone)]
pub struct ShellContext {
    pub engine: EngineHandle,
    pub rt_handle: tokio::runtime::Handle,
    pub current_node: Arc<RwLock<Option<String>>>,
}

pub enum ResolveResult {
    Found(String),
    Ambiguous(Vec<String>),
    NotFound,
}

impl ShellContext {
    pub fn new(
        engine: EngineHandle,
        rt_handle: tokio::runtime::Handle,
        current_node: Arc<RwLock<Option<String>>>,
    ) -> Self {
        Self {
            engine,
            rt_handle,
            current_node,
        }
    }

    pub fn get_feature_provider(&self, lang: Language) -> Option<Arc<dyn LanguageFeatureProvider>> {
        self.engine.get_feature_provider(lang)
    }

    pub fn current_fqn(&self) -> Option<String> {
        self.current_node.read().unwrap().clone()
    }

    pub fn set_current_fqn(&self, fqn: Option<String>) {
        *self.current_node.write().unwrap() = fqn;
    }

    /// Helper to get graph snapshot synchronously
    pub fn graph(&self) -> CodeGraph {
        self.rt_handle.block_on(self.engine.graph())
    }

    /// Helper to execute query synchronously
    pub fn execute_query(
        &self,
        query: &GraphQuery,
    ) -> naviscope_core::error::Result<naviscope_core::query::QueryResult> {
        self.rt_handle.block_on(self.engine.query(query))
    }

    /// Resolves a user input path (absolute FQN, relative path, or fuzzy name) to a concrete FQN.
    pub fn resolve_node(&self, target: &str) -> ResolveResult {
        // 1. Handle special paths
        if let Some(result) = self.resolve_special_path(target) {
            return result;
        }

        let curr = self.current_fqn();
        let graph = self.graph();

        // 2. Handle Parent (..) navigation
        if let Some(result) = Self::resolve_parent(target, &curr, &graph) {
            return result;
        }

        // 3. Try Exact Match (Absolute FQN)
        if let Some(result) = Self::resolve_exact_match(target, &graph) {
            return result;
        }

        // 4. Try Relative Path from current context
        if let Some(curr_fqn) = &curr {
            // Join current FQN and target
            let separator = if curr_fqn.contains("::") { "::" } else { "." };
            let joined = format!("{}{}{}", curr_fqn, separator, target);
            if let Some(result) = Self::resolve_exact_match(&joined, &graph) {
                return result;
            }
        }

        // 5. Try Child Lookup (Immediate / Fuzzy)
        self.resolve_child_lookup(target, &curr, &graph)
    }

    /// Handles special paths like "/" (root) and "root".
    fn resolve_special_path(&self, target: &str) -> Option<ResolveResult> {
        if target == "/" || target == "root" {
            let graph = self.graph();
            use naviscope_core::model::graph::NodeKind;

            // Find all Project nodes
            let project_nodes: Vec<_> = graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &graph.topology()[idx];
                    if node.kind() == NodeKind::Project {
                        Some(node.fqn().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            match project_nodes.len() {
                1 => Some(ResolveResult::Found(project_nodes[0].clone())),
                0 => Some(ResolveResult::Found("".to_string())), // Empty graph or virtual root
                _ => Some(ResolveResult::Ambiguous(project_nodes)),
            }
        } else {
            None
        }
    }

    /// Handles parent navigation ("..").
    fn resolve_parent(
        target: &str,
        current_fqn: &Option<String>,
        graph: &CodeGraph,
    ) -> Option<ResolveResult> {
        if target != ".." {
            return None;
        }

        if let Some(c) = current_fqn {
            // Graph-based parent lookup
            if let Some(&idx) = graph.fqn_map().get(c.as_str()) {
                let mut incoming = graph
                    .topology()
                    .neighbors_directed(idx, petgraph::Direction::Incoming)
                    .detach();

                while let Some((edge_idx, neighbor_idx)) = incoming.next(graph.topology()) {
                    let edge = &graph.topology()[edge_idx];
                    if edge.edge_type == naviscope_core::model::graph::EdgeType::Contains {
                        if let Some(parent_node) = graph.topology().node_weight(neighbor_idx) {
                            return Some(ResolveResult::Found(parent_node.fqn().to_string()));
                        }
                    }
                }
            }

            // Fallback: String manipulation
            if let Some(last_dot) = c.rfind('.') {
                return Some(ResolveResult::Found(c[0..last_dot].to_string()));
            } else if c.contains("::") {
                let parts: Vec<&str> = c.split("::").collect();
                if parts.len() > 1 {
                    let parent = parts[..parts.len() - 1].join("::");
                    if parent == "module" {
                        return Some(ResolveResult::Found("".to_string())); // Root
                    }
                    return Some(ResolveResult::Found(parent));
                }
            }
            Some(ResolveResult::Found("".to_string())) // Root
        } else {
            Some(ResolveResult::Found("".to_string())) // Already at root
        }
    }

    /// Tries exact match against absolute FQN.
    fn resolve_exact_match(target: &str, graph: &CodeGraph) -> Option<ResolveResult> {
        if graph.fqn_map().contains_key(target) {
            Some(ResolveResult::Found(target.to_string()))
        } else {
            None
        }
    }

    /// Tries child lookup with exact and fuzzy name matching.
    fn resolve_child_lookup(
        &self,
        target: &str,
        current_fqn: &Option<String>,
        _graph: &CodeGraph, // We use self.execute_query instead
    ) -> ResolveResult {
        let children_query = GraphQuery::Ls {
            fqn: current_fqn.clone(),
            kind: vec![],
            modifiers: vec![],
        };

        if let Ok(res) = self.execute_query(&children_query) {
            // First pass: Exact Name Match
            let exact_matches = Self::find_exact_name_match(target, &res.nodes);
            if !exact_matches.is_empty() {
                if exact_matches.len() == 1 {
                    return ResolveResult::Found(exact_matches[0].clone());
                } else {
                    return ResolveResult::Ambiguous(exact_matches);
                }
            }

            // Second pass: Fuzzy Name Match (e.g. method name without signature)
            let fuzzy_matches = Self::find_fuzzy_name_match(target, &res.nodes);
            if !fuzzy_matches.is_empty() {
                if fuzzy_matches.len() == 1 {
                    return ResolveResult::Found(fuzzy_matches[0].clone());
                } else {
                    return ResolveResult::Ambiguous(fuzzy_matches);
                }
            }
        }

        ResolveResult::NotFound
    }

    /// Finds nodes with exact name match.
    fn find_exact_name_match(target: &str, nodes: &[GraphNode]) -> Vec<String> {
        nodes
            .iter()
            .filter(|n| {
                let name = n.name();
                // Handle display names with trailing slash
                let clean_name = name.trim_end_matches('/');
                clean_name == target
            })
            .map(|n| n.fqn().to_string())
            .collect()
    }

    /// Finds nodes with fuzzy name match (e.g. method name without signature).
    fn find_fuzzy_name_match(target: &str, nodes: &[GraphNode]) -> Vec<String> {
        nodes
            .iter()
            .filter(|n| {
                let name = n.name();
                let clean_name = name.trim_end_matches('/');
                clean_name.split('(').next().unwrap_or("") == target
            })
            .map(|n| n.fqn().to_string())
            .collect()
    }
}
