use crate::graph::CodeGraph;
use naviscope_api::models::SymbolResolution;
use naviscope_api::models::graph::DisplayGraphNode;
use naviscope_api::models::symbol::{FqnId, Range};
use tree_sitter::Tree;

pub trait SymbolResolveService: Send + Sync {
    fn resolve_at(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        byte_col: usize,
        index: &dyn CodeGraph,
    ) -> Option<SymbolResolution>;
}

pub trait SymbolQueryService: Send + Sync {
    fn find_matches(&self, index: &dyn CodeGraph, res: &SymbolResolution) -> Vec<FqnId>;
    fn resolve_type_of(
        &self,
        index: &dyn CodeGraph,
        res: &SymbolResolution,
    ) -> Vec<SymbolResolution>;
    fn find_implementations(&self, index: &dyn CodeGraph, res: &SymbolResolution) -> Vec<FqnId>;
}

pub trait LspSyntaxService: Send + Sync {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree>;
    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DisplayGraphNode>;
    fn find_occurrences(&self, source: &str, tree: &Tree, target: &SymbolResolution) -> Vec<Range>;
}

pub trait ReferenceCheckService: Send + Sync {
    fn is_reference_to(
        &self,
        graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool;

    fn is_subtype(&self, graph: &dyn CodeGraph, sub: &str, sup: &str) -> bool {
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

                let parents = graph.get_neighbors(
                    current,
                    crate::graph::Direction::Outgoing,
                    Some(EdgeType::InheritsFrom),
                );
                for parent in parents {
                    if visited.insert(parent) {
                        queue.push_back(parent);
                    }
                }

                let interfaces = graph.get_neighbors(
                    current,
                    crate::graph::Direction::Outgoing,
                    Some(EdgeType::Implements),
                );
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

pub trait SemanticCap:
    SymbolResolveService + SymbolQueryService + LspSyntaxService + ReferenceCheckService + Send + Sync
{
}

impl<T> SemanticCap for T where
    T: SymbolResolveService
        + SymbolQueryService
        + LspSyntaxService
        + ReferenceCheckService
        + Send
        + Sync
{
}
