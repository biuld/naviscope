use super::language::Language;
use super::symbol::Range;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Package,
    Module,
    Class,
    Interface,
    Enum,
    Annotation,
    Method,
    Constructor,
    Field,
    Variable,
    // Build Specific
    Project,
    Dependency,
    Task,
    Plugin,
    // Extension
    Custom(#[schemars(with = "String")] String),
}

impl From<&str> for NodeKind {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "package" => NodeKind::Package,
            "module" => NodeKind::Module,
            "class" => NodeKind::Class,
            "interface" => NodeKind::Interface,
            "enum" => NodeKind::Enum,
            "annotation" => NodeKind::Annotation,
            "method" => NodeKind::Method,
            "constructor" => NodeKind::Constructor,
            "field" => NodeKind::Field,
            "variable" => NodeKind::Variable,
            "project" => NodeKind::Project,
            "dependency" => NodeKind::Dependency,
            "task" => NodeKind::Task,
            "plugin" => NodeKind::Plugin,
            _ => NodeKind::Custom(s.to_string()),
        }
    }
}

impl ToString for NodeKind {
    fn to_string(&self) -> String {
        match self {
            NodeKind::Package => "package".to_string(),
            NodeKind::Module => "module".to_string(),
            NodeKind::Class => "class".to_string(),
            NodeKind::Interface => "interface".to_string(),
            NodeKind::Enum => "enum".to_string(),
            NodeKind::Annotation => "annotation".to_string(),
            NodeKind::Method => "method".to_string(),
            NodeKind::Constructor => "constructor".to_string(),
            NodeKind::Field => "field".to_string(),
            NodeKind::Variable => "variable".to_string(),
            NodeKind::Project => "project".to_string(),
            NodeKind::Dependency => "dependency".to_string(),
            NodeKind::Task => "task".to_string(),
            NodeKind::Plugin => "plugin".to_string(),
            NodeKind::Custom(s) => s.to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum EdgeType {
    // Structural relationships
    Contains,
    // Inheritance/Implementation
    InheritsFrom,
    Implements,
    // Usage/Reference
    TypedAs,
    DecoratedBy,
    // Build system relationships
    UsesDependency,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct GraphEdge {
    pub edge_type: EdgeType,
}

impl GraphEdge {
    pub fn new(edge_type: EdgeType) -> Self {
        Self { edge_type }
    }
}

use super::symbol::{FqnId, Symbol};
use lasso::Reader;
use std::any::Any;
use std::fmt::Debug;

/// Trait for language-specific metadata.
pub trait NodeMetadata: Send + Sync + Debug {
    /// Cast to Any for downcasting to concrete types.
    fn as_any(&self) -> &dyn Any;
}

/// Default empty metadata implementation.
#[derive(Debug, Clone)]
pub struct EmptyMetadata;

impl NodeMetadata for EmptyMetadata {
    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum NodeSource {
    /// Defined in the current project (source code available)
    Project,
    /// External dependency (library, vendor code)
    External,
    /// Language builtin / Primitive type
    Builtin,
}

impl Default for NodeSource {
    fn default() -> Self {
        Self::Project
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq, Hash, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionStatus {
    /// Just a placeholder (name and ID known, metadata may be empty)
    Unresolved,
    /// Structure known from bytecode or partial scan (stubs available)
    Stubbed,
    /// Full details known from source code or complete parsing
    #[default]
    Resolved,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    /// Unique Identifier (Structured FQN)
    pub id: FqnId,
    /// Short display name (Symbol)
    pub name: Symbol,
    /// Abstract categorization
    pub kind: NodeKind,
    /// Language identifier (Symbol)
    pub lang: Symbol,
    /// Source origin
    pub source: NodeSource,
    /// Current resolution depth/state
    pub status: ResolutionStatus,
    /// Physical Location
    pub location: Option<super::symbol::InternedLocation>,
    /// Extension metadata
    pub metadata: Arc<dyn NodeMetadata>,
}

impl Default for GraphNode {
    fn default() -> Self {
        Self {
            id: FqnId(0),
            name: Symbol(lasso::Spur::default()),
            kind: NodeKind::Custom("unknown".to_string()),
            lang: Symbol(lasso::Spur::default()),
            source: NodeSource::Project,
            status: ResolutionStatus::Resolved,
            location: None,
            metadata: Arc::new(EmptyMetadata),
        }
    }
}

impl GraphNode {
    pub fn language<'a>(&self, rodeo: &'a dyn Reader) -> Language {
        Language::new(rodeo.resolve(&self.lang.0).to_string())
    }

    pub fn name<'a>(&self, rodeo: &'a dyn Reader) -> &'a str {
        rodeo.resolve(&self.name.0)
    }

    pub fn kind(&self) -> NodeKind {
        self.kind.clone()
    }

    pub fn path<'a>(&self, rodeo: &'a impl Reader) -> Option<&'a str> {
        self.location.as_ref().map(|l| rodeo.resolve(&l.path.0))
    }

    pub fn range(&self) -> Option<&Range> {
        self.location.as_ref().map(|l| &l.range)
    }

    pub fn name_range(&self) -> Option<&Range> {
        self.location
            .as_ref()
            .and_then(|l| l.selection_range.as_ref())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct DisplaySymbolLocation {
    pub path: String,
    pub range: Range,
    #[serde(default)]
    pub selection_range: Option<Range>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct DisplayGraphNode {
    pub id: String,
    pub name: String,
    pub kind: NodeKind,
    pub lang: String,
    #[serde(default)]
    pub source: NodeSource,
    #[serde(default)]
    pub status: ResolutionStatus,
    pub location: Option<DisplaySymbolLocation>,

    // Rendering fields
    pub detail: Option<String>,
    pub signature: Option<String>,
    #[serde(default)]
    pub modifiers: Vec<String>,

    // Hierarchy support
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<DisplayGraphNode>>,
}

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    /// List members or structure (Rich Listing)
    Ls {
        /// Target node FQN, defaults to project modules if null
        fqn: Option<String>,
        #[serde(default)]
        kind: Vec<NodeKind>,
        #[serde(default)]
        sources: Vec<NodeSource>,
        #[serde(default)]
        modifiers: Vec<String>,
    },

    /// Search for symbols
    Find {
        pattern: String,
        #[serde(default)]
        kind: Vec<NodeKind>,
        #[serde(default)]
        sources: Vec<NodeSource>,
        #[serde(default = "default_limit")]
        limit: usize,
    },

    /// Inspect node details (Source & Metadata)
    Cat { fqn: String },

    /// Find dependencies (outgoing) or dependents (incoming)
    Deps {
        fqn: String,
        /// If true, find incoming dependencies (who depends on me).
        /// If false (default), find outgoing dependencies (who do I depend on).
        #[serde(default)]
        rev: bool,
        #[serde(default)]
        edge_types: Vec<EdgeType>,
    },
}

fn default_limit() -> usize {
    20
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryResultEdge {
    #[serde(with = "super::util::serde_arc_str")]
    pub from: Arc<str>,
    #[serde(with = "super::util::serde_arc_str")]
    pub to: Arc<str>,
    pub data: GraphEdge,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct QueryResult {
    pub nodes: Vec<DisplayGraphNode>,
    pub edges: Vec<QueryResultEdge>,
}

impl QueryResult {
    pub fn new(nodes: Vec<DisplayGraphNode>, edges: Vec<QueryResultEdge>) -> Self {
        Self { nodes, edges }
    }
}
