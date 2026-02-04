use super::graph::{DisplayGraphNode, NodeKind};
use super::language::Language;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Symbol(pub lasso::Spur);

impl JsonSchema for Symbol {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("Symbol")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        u32::json_schema(generator)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum NodeId {
    Flat(String),
    Structured(Vec<(super::graph::NodeKind, String)>),
}

impl From<String> for NodeId {
    fn from(s: String) -> Self {
        NodeId::Flat(s)
    }
}

impl From<&str> for NodeId {
    fn from(s: &str) -> Self {
        NodeId::Flat(s.to_string())
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            NodeId::Flat(s) => write!(f, "{}", s),
            NodeId::Structured(parts) => {
                for (i, (kind, name)) in parts.iter().enumerate() {
                    if i > 0 {
                        match kind {
                            super::graph::NodeKind::Method
                            | super::graph::NodeKind::Constructor
                            | super::graph::NodeKind::Field => {
                                write!(f, "#")?;
                            }
                            _ => {
                                write!(f, ".")?;
                            }
                        }
                    }
                    write!(f, "{}", name)?;
                }
                Ok(())
            }
        }
    }
}

impl NodeId {
    pub fn as_str(&self) -> &str {
        match self {
            NodeId::Flat(s) => s.as_str(),
            NodeId::Structured(_) => "structured_id",
        }
    }
}

pub type SymbolAtom = Symbol;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FqnId(pub u32);

impl JsonSchema for FqnId {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        std::borrow::Cow::Borrowed("FqnId")
    }

    fn json_schema(generator: &mut schemars::SchemaGenerator) -> schemars::Schema {
        u32::json_schema(generator)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct FqnNode {
    pub parent: Option<FqnId>,
    pub name: Symbol,
    pub kind: NodeKind,
}

pub trait FqnReader {
    fn resolve_node(&self, id: FqnId) -> Option<FqnNode>;
    fn resolve_atom(&self, atom: Symbol) -> &str;
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, JsonSchema)]
pub struct Range {
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Default for Range {
    fn default() -> Self {
        Self {
            start_line: 0,
            start_col: 0,
            end_line: 0,
            end_col: 0,
        }
    }
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

impl SymbolResolution {
    pub fn fqn(&self) -> Option<&str> {
        match self {
            SymbolResolution::Local(_, _) => None,
            SymbolResolution::Precise(fqn, _) => Some(fqn),
            SymbolResolution::Global(fqn) => Some(fqn),
        }
    }
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

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct InternedLocation {
    pub path: Symbol,
    pub range: Range,
    #[serde(default)]
    pub selection_range: Option<Range>,
}

impl InternedLocation {
    pub fn to_display(&self, fqns: &dyn FqnReader) -> super::graph::DisplaySymbolLocation {
        super::graph::DisplaySymbolLocation {
            path: fqns.resolve_atom(self.path).to_string(),
            range: self.range,
            selection_range: self.selection_range,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferenceQuery {
    pub resolution: SymbolResolution,
    pub language: Language,
    pub include_declaration: bool,
}

#[derive(Debug, Clone)]
pub struct CallHierarchyIncomingCall {
    pub from: DisplayGraphNode,
    pub from_ranges: Vec<Range>,
}

#[derive(Debug, Clone)]
pub struct CallHierarchyOutgoingCall {
    pub to: DisplayGraphNode,
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
