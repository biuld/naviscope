use crate::JavaPlugin;
use naviscope_plugin::{LspSyntaxService, ReferenceCheckService};

impl LspSyntaxService for JavaPlugin {
    fn parse(
        &self,
        source: &str,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> Option<tree_sitter::Tree> {
        crate::lsp::JavaLspService::new(self.parser.clone()).parse(source, old_tree)
    }

    fn extract_symbols(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
    ) -> Vec<naviscope_api::models::graph::DisplayGraphNode> {
        crate::lsp::JavaLspService::new(self.parser.clone()).extract_symbols(tree, source)
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &tree_sitter::Tree,
        target: &naviscope_api::models::SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::Range> {
        crate::lsp::JavaLspService::new(self.parser.clone()).find_occurrences(source, tree, target)
    }
}

impl ReferenceCheckService for JavaPlugin {
    fn is_reference_to(
        &self,
        graph: &dyn naviscope_plugin::CodeGraph,
        candidate: &naviscope_api::models::SymbolResolution,
        target: &naviscope_api::models::SymbolResolution,
    ) -> bool {
        self.type_system.is_reference_to(graph, candidate, target)
    }

    fn is_subtype(&self, graph: &dyn naviscope_plugin::CodeGraph, sub: &str, sup: &str) -> bool {
        self.type_system.is_subtype(graph, sub, sup)
    }
}
