use crate::error::Result;
use crate::model::graph::{GraphNode, NodeKind, Range};
use std::path::Path;
use tree_sitter::Tree;

// Re-export from API
pub use naviscope_api::models::{DocumentSymbol, SymbolIntent, SymbolResolution, matches_intent};

pub trait LspParser: Send + Sync {
    fn parse(
        &self,
        source: &str,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> Option<tree_sitter::Tree>;
    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DocumentSymbol>;
    /// Maps a language-specific symbol kind string to an LSP SymbolKind
    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind;

    /// Find occurrences of a symbol within a single file's AST.
    /// This is the "Micro" part of the Discovery Engine.
    fn find_occurrences(&self, source: &str, tree: &Tree, target: &SymbolResolution) -> Vec<Range>;
}

/// Result of a global file parsing for indexing.
#[derive(Clone)]
pub struct GlobalParseResult {
    pub package_name: Option<String>,
    pub imports: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub relations: Vec<(String, String, crate::model::graph::EdgeType, Option<Range>)>,
    pub source: Option<String>,
    pub tree: Option<Tree>,
    pub identifiers: Vec<String>,
}

/// Trait for parsers that provide data for the global code knowledge graph.
pub trait IndexParser: Send + Sync {
    fn parse_file(&self, source_code: &str, file_path: Option<&Path>) -> Result<GlobalParseResult>;
}

pub mod utils;
