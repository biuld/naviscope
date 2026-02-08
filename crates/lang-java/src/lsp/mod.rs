mod references;
mod symbols;

use crate::parser::JavaParser;
use naviscope_api::models::SymbolResolution;
use naviscope_api::models::graph::{DisplayGraphNode, NodeKind};
use naviscope_plugin::LspService;
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

impl LspService for JavaLspService {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        self.parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DisplayGraphNode> {
        symbols::extract_symbols(&self.parser, tree, source)
    }

    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind {
        use lsp_types::SymbolKind;
        match kind {
            NodeKind::Class => SymbolKind::CLASS,
            NodeKind::Interface => SymbolKind::INTERFACE,
            NodeKind::Enum => SymbolKind::ENUM,
            NodeKind::Annotation => SymbolKind::INTERFACE,
            NodeKind::Method => SymbolKind::METHOD,
            NodeKind::Constructor => SymbolKind::CONSTRUCTOR,
            NodeKind::Field => SymbolKind::FIELD,
            NodeKind::Package => SymbolKind::PACKAGE,
            _ => SymbolKind::VARIABLE,
        }
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &Tree,
        target: &SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::Range> {
        references::find_occurrences(&self.parser, source, tree, target)
    }
}
