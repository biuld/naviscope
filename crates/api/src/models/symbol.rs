use super::graph::NodeKind;
use super::language::Language;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub struct Range {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Range {
    pub fn contains(&self, line: usize, col: usize) -> bool {
        if line < self.start_line || line > self.end_line {
            return false;
        }
        if line == self.start_line && col < self.start_col {
            return false;
        }
        if line == self.end_line && col > self.end_col {
            return false;
        }
        true
    }
}

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
        SymbolIntent::Type => matches!(
            node_kind,
            NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation
        ),
        SymbolIntent::Method => matches!(node_kind, NodeKind::Method | NodeKind::Constructor),
        SymbolIntent::Field => matches!(node_kind, NodeKind::Field),
        SymbolIntent::Variable => false,
        SymbolIntent::Unknown => true,
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SymbolResolution {
    Local(Range, Option<String>), // Range of declaration, and optional type name
    Precise(String, SymbolIntent),
    Global(String),
}

#[derive(Debug, Clone)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: NodeKind,
    pub range: Range,
    pub selection_range: Range,
    pub children: Vec<DocumentSymbol>,
}

// --- New Core API Types ---

#[derive(Debug, Clone)]
pub struct PositionContext {
    pub uri: String,
    pub line: u32,
    pub char: u32,
    pub content: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SymbolQuery {
    pub resolution: SymbolResolution,
    // Note: Language is usually needed but we might infer it or pass it.
    // For now we keep it simple or use strings.
    // But Language enum is in core/project/source.rs.
    // We should probably move Language enum to API models too if it's part of the API.
    // Let's assume passed as generic or String or enum moved.
    pub language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolLocation {
    pub path: PathBuf,
    pub range: Range,
    pub fqn: String,
    // Add node_index? Better avoid if possible to keep it detached from graph internals.
    // But for efficiency `find_definitions` returned `SymbolLocation` which had node_index.
    // Let's keep node_index out for public API if possible.
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: NodeKind,
    pub detail: Option<String>,
    pub location: SymbolLocation,
    pub signature: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReferenceQuery {
    pub resolution: SymbolResolution,
    pub language: Language,
    pub include_declaration: bool,
}

#[derive(Debug, Clone)]
pub struct CallHierarchyItem {
    pub name: String,
    pub kind: NodeKind,
    pub detail: Option<String>,
    pub uri: String,
    pub range: Range,
    pub selection_range: Range,
    pub id: String, // Added id for cross-request tracking (e.g. FQN)
}

#[derive(Debug, Clone)]
pub struct CallHierarchyIncomingCall {
    pub from: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}

#[derive(Debug, Clone)]
pub struct CallHierarchyOutgoingCall {
    pub to: CallHierarchyItem,
    pub from_ranges: Vec<Range>,
}
