use crate::cap::ReferenceCheckService;
use crate::graph::{CodeGraph, Direction};
use naviscope_api::models::SymbolResolution;
use naviscope_api::models::graph::EdgeType;

pub struct NoOpReferenceCheckService;

impl ReferenceCheckService for NoOpReferenceCheckService {
    fn is_reference_to(
        &self,
        _graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool {
        candidate == target
    }

    fn is_subtype(&self, graph: &dyn CodeGraph, sub: &str, sup: &str) -> bool {
        if sub == sup {
            return true;
        }

        let sub_ids = graph.resolve_fqn(sub);
        let sup_ids = graph.resolve_fqn(sup);
        if sub_ids.is_empty() || sup_ids.is_empty() {
            return false;
        }

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

                let parents =
                    graph.get_neighbors(current, Direction::Outgoing, Some(EdgeType::InheritsFrom));
                for parent in parents {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }

                let interfaces =
                    graph.get_neighbors(current, Direction::Outgoing, Some(EdgeType::Implements));
                for interface in interfaces {
                    if visited.insert(interface) {
                        queue.push_back(interface);
                    }
                }
            }
        }

        false
    }
}
