use crate::graph::CodeGraph;
use naviscope_api::models::SymbolResolution;
use std::sync::Arc;

/// Unified interface for language-specific type systems and semantic reasoning.
pub trait TypeSystem: Send + Sync {
    /// Checks if a candidate resolution is a semantically valid reference to the target resolution.
    /// This handles subtyping, interface implementations, and member overrides.
    fn is_reference_to(
        &self,
        graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool;

    /// Checks if `sub` is a subtype of `sup` in the given code graph.
    /// Default implementation uses BFS to traverse InheritsFrom and Implements edges.
    fn is_subtype(&self, graph: &dyn crate::graph::CodeGraph, sub: &str, sup: &str) -> bool {
        if sub == sup {
            return true;
        }

        let sub_ids = graph.resolve_fqn(sub);
        let sup_ids = graph.resolve_fqn(sup);

        if sub_ids.is_empty() || sup_ids.is_empty() {
            return false;
        }

        use naviscope_api::models::graph::EdgeType;
        use std::collections::{HashSet, VecDeque};

        for &sub_id in &sub_ids {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();
            queue.push_back(sub_id);
            visited.insert(sub_id);

            while let Some(current) = queue.pop_front() {
                if sup_ids.contains(&current) {
                    return true;
                }

                // Check parents (InheritsFrom / Implements)
                let parents = graph.get_neighbors(
                    current,
                    crate::graph::Direction::Outgoing,
                    Some(EdgeType::InheritsFrom),
                );
                for p in parents {
                    if !visited.contains(&p) {
                        visited.insert(p);
                        queue.push_back(p);
                    }
                }

                let interfaces = graph.get_neighbors(
                    current,
                    crate::graph::Direction::Outgoing,
                    Some(EdgeType::Implements),
                );
                for i in interfaces {
                    if !visited.contains(&i) {
                        visited.insert(i);
                        queue.push_back(i);
                    }
                }
            }
        }

        false
    }
}

/// Pointer type for the type system.
pub type TypeSystemPtr = Arc<dyn TypeSystem>;

/// A simple no-op type system that only performs exact equality checks.
/// Useful as a fallback for languages that don't yet have full type system support.
pub struct NoOpTypeSystem;

impl TypeSystem for NoOpTypeSystem {
    fn is_reference_to(
        &self,
        _graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool {
        // Fallback to basic equality check
        candidate == target
    }
}
