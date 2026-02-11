mod references;
mod symbols;
pub mod type_system;

use crate::parser::JavaParser;
use naviscope_api::models::SymbolResolution;
use naviscope_api::models::graph::DisplayGraphNode;
use naviscope_plugin::LspSyntaxService;
use std::sync::Arc;
use tree_sitter::Tree;

pub struct JavaLspService {
    parser: Arc<JavaParser>,
}

impl JavaLspService {
    pub fn new(parser: Arc<JavaParser>) -> Self {
        Self { parser }
    }
}

impl LspSyntaxService for JavaLspService {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        self.parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DisplayGraphNode> {
        symbols::extract_symbols(&self.parser, tree, source)
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &Tree,
        target: &SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::Range> {
        // In the fast syntactic path, we don't strictly need the full type system,
        // but we could use it for early filtering if needed.
        // For now, we perform the syntactic scan.
        references::find_occurrences(&self.parser, source, tree, target)
    }
}
