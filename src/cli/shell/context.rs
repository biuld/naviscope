use std::sync::{Arc, RwLock};
use naviscope::index::{Naviscope, CodeGraph};
use naviscope::query::{QueryEngine, GraphQuery};
use naviscope::model::graph::GraphNode;

#[derive(Clone)]
pub struct ShellContext {
    pub naviscope: Arc<RwLock<Naviscope>>,
    pub current_node: Arc<RwLock<Option<String>>>,
}

pub enum ResolveResult {
    Found(String),
    Ambiguous(Vec<String>),
    NotFound,
}

impl ShellContext {
    pub fn new(naviscope: Arc<RwLock<Naviscope>>, current_node: Arc<RwLock<Option<String>>>) -> Self {
        Self {
            naviscope,
            current_node,
        }
    }

    pub fn current_fqn(&self) -> Option<String> {
        self.current_node.read().unwrap().clone()
    }

    pub fn set_current_fqn(&self, fqn: Option<String>) {
        *self.current_node.write().unwrap() = fqn;
    }

    /// Resolves a user input path (absolute FQN, relative path, or fuzzy name) to a concrete FQN.
    pub fn resolve_node(&self, target: &str) -> ResolveResult {
        // 1. Handle special paths
        if let Some(result) = Self::resolve_special_path(target) {
            return result;
        }
        
        let curr = self.current_fqn();
        let engine_guard = self.naviscope.read().unwrap();
        let graph = engine_guard.graph();

        // 2. Handle Parent (..) navigation
        if let Some(result) = Self::resolve_parent(target, &curr, graph) {
            return result;
        }

        // 3. Try Exact Match (Absolute FQN)
        if let Some(result) = Self::resolve_exact_match(target, graph) {
            return result;
        }

        // 4. Try Child Lookup (Relative / Fuzzy)
        Self::resolve_child_lookup(target, &curr, graph)
    }

    /// Handles special paths like "/" (root).
    fn resolve_special_path(target: &str) -> Option<ResolveResult> {
        if target == "/" {
            Some(ResolveResult::Found("".to_string())) // Marker for root
        } else {
            None
        }
    }

    /// Handles parent navigation ("..").
    fn resolve_parent(target: &str, current_fqn: &Option<String>, graph: &CodeGraph) -> Option<ResolveResult> {
        if target != ".." {
            return None;
        }

        if let Some(c) = current_fqn {
            // Graph-based parent lookup
            if let Some(&idx) = graph.fqn_map.get(c) {
                let mut incoming = graph.topology
                    .neighbors_directed(idx, petgraph::Direction::Incoming)
                    .detach();
                
                while let Some((edge_idx, neighbor_idx)) = incoming.next(&graph.topology) {
                    let edge = &graph.topology[edge_idx];
                    if edge.edge_type == naviscope::model::graph::EdgeType::Contains {
                        if let Some(parent_node) = graph.topology.node_weight(neighbor_idx) {
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
                    let parent = parts[..parts.len()-1].join("::");
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
        if graph.fqn_map.contains_key(target) {
            Some(ResolveResult::Found(target.to_string()))
        } else {
            None
        }
    }

    /// Tries child lookup with exact and fuzzy name matching.
    fn resolve_child_lookup(target: &str, current_fqn: &Option<String>, graph: &CodeGraph) -> ResolveResult {
        let query_engine = QueryEngine::new(graph);
        let children_query = GraphQuery::Ls {
            fqn: current_fqn.clone(),
            kind: vec![],
            modifiers: vec![],
        };

        if let Ok(res) = query_engine.execute(&children_query) {
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
        nodes.iter()
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
        nodes.iter()
            .filter(|n| {
                let name = n.name();
                let clean_name = name.trim_end_matches('/');
                clean_name.split('(').next().unwrap_or("") == target
            })
            .map(|n| n.fqn().to_string())
            .collect()
    }
}