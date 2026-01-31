use super::graph::NodeKind;
use super::language::Language;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

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
    pub language: Language,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct SymbolLocation {
    #[serde(with = "super::util::serde_arc_path")]
    #[schemars(with = "String")]
    pub path: Arc<Path>,
    pub range: Range,
    /// Range of the identifier/name (for precise navigation)
    #[serde(default)]
    pub selection_range: Option<Range>,
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

// --- Type System ---

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum TypeRef {
    /// Unresolved or primitive type name (e.g., "int", "void", "List<T>")
    Raw(String),

    /// Resolved reference to a Type definition node (FQN)
    Id(String),

    /// Generic instantiation (e.g., List<String>)
    Generic {
        base: Box<TypeRef>,
        args: Vec<TypeRef>,
    },

    /// Array type (e.g., String[])
    Array {
        element: Box<TypeRef>,
        dimensions: usize,
    },

    /// Wildcard type (e.g., ? extends Number)
    Wildcard {
        bound: Option<Box<TypeRef>>,
        is_upper_bound: bool, // true: extends, false: super
    },

    Unknown,
}

impl TypeRef {
    /// Helper to create a Raw type
    pub fn raw(s: impl Into<String>) -> Self {
        TypeRef::Raw(s.into())
    }

    /// Helper to create an Id type
    pub fn id(s: impl Into<String>) -> Self {
        TypeRef::Id(s.into())
    }
}

impl Default for TypeRef {
    fn default() -> Self {
        TypeRef::Unknown
    }
}
