use crate::model::graph::{GraphNode, NodeKind, Range};
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

pub fn matches_intent(node_kind: &NodeKind, intent: SymbolIntent) -> bool {
    match intent {
        SymbolIntent::Type => matches!(node_kind, NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation),
        SymbolIntent::Method => matches!(node_kind, NodeKind::Method | NodeKind::Constructor),
        SymbolIntent::Field => matches!(node_kind, NodeKind::Field),
        SymbolIntent::Variable => false, // Graph nodes are rarely variables, usually only Definitions
        SymbolIntent::Unknown => true,
    }
}

#[derive(Debug, Clone)]
pub enum SymbolResolution {
    Local(Range, Option<String>), // Range of declaration, and optional type name
    Precise(String, SymbolIntent),
}

pub trait LspParser: Send + Sync {
    fn parse(&self, source: &str, old_tree: Option<&tree_sitter::Tree>) -> Option<tree_sitter::Tree>;
    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<DocumentSymbol>;
    /// Maps a language-specific symbol kind string to an LSP SymbolKind
    fn symbol_kind(&self, kind: &NodeKind) -> tower_lsp::lsp_types::SymbolKind;
}

/// Result of a global file parsing for indexing.
pub struct GlobalParseResult {
    pub package_name: Option<String>,
    pub imports: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub relations: Vec<(String, String, crate::model::graph::EdgeType, Option<Range>)>,
    pub source: Option<String>,
    pub tree: Option<Tree>,
}

/// Trait for parsers that provide data for the global code knowledge graph.
pub trait IndexParser: Send + Sync {
    fn parse_file(&self, source_code: &str, file_path: Option<&Path>) -> Result<GlobalParseResult>;
}

#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: NodeKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

pub mod gradle;
pub mod java;
pub mod queries;
pub mod utils;
