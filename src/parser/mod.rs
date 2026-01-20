use crate::model::graph::{GraphNode, Range};
use tree_sitter::Tree;
use std::path::Path;
use crate::error::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SymbolIntent {
    Type,
    Method,
    Field,
    Variable,
    Unknown,
}

pub fn matches_intent(node_kind: &str, intent: SymbolIntent) -> bool {
    match intent {
        SymbolIntent::Type => {
            node_kind == "class"
                || node_kind == "interface"
                || node_kind == "enum"
                || node_kind == "annotation"
        }
        SymbolIntent::Method => node_kind == "method" || node_kind == "constructor",
        SymbolIntent::Field => node_kind == "field",
        SymbolIntent::Variable => node_kind == "variable" || node_kind == "parameter",
        SymbolIntent::Unknown => true,
    }
}

#[derive(Debug, Clone)]
pub enum SymbolResolution {
    Local(Range),
    Precise(String, SymbolIntent),
    Heuristic(String, SymbolIntent),
}

pub trait LspParser: Send + Sync {
    fn parse(&self, source: &str, old_tree: Option<&tree_sitter::Tree>) -> Option<tree_sitter::Tree>;
    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DocumentSymbol>;
    /// Maps a language-specific symbol kind string to an LSP SymbolKind
    fn symbol_kind(&self, kind: &str) -> tower_lsp::lsp_types::SymbolKind;
}

/// Result of a global file parsing for indexing.
pub struct GlobalParseResult {
    pub package_name: Option<String>,
    pub imports: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub relations: Vec<(String, String, crate::model::graph::EdgeType, Option<Range>)>,
}

/// Trait for parsers that provide data for the global code knowledge graph.
pub trait IndexParser: Send + Sync {
    fn parse_file(&self, source_code: &str, file_path: Option<&Path>) -> Result<GlobalParseResult>;
}

#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: String,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

pub mod gradle;
pub mod java;
pub mod queries;
pub mod utils;
