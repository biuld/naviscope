use super::language::Language;
use super::symbol::Range;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::path::Path;
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

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct GraphNode {
    /// Unique Identifier (FQN)
    #[serde(with = "super::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub id: Arc<str>,
    /// Short display name
    #[schemars(with = "String")]
    pub name: SmolStr,
    /// Abstract categorization
    pub kind: NodeKind,
    /// Language identifier ("java", "rust", "buildfile")
    #[serde(with = "super::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub lang: Arc<str>,
    /// Physical Location
    pub location: Option<super::symbol::SymbolLocation>,
    /// Extension metadata
    #[serde(default)]
    pub metadata: serde_json::Value,
}

impl GraphNode {
    pub fn language(&self) -> Language {
        match self.lang.as_ref() {
            "java" => Language::Java,
            "rust" => Language::Rust,
            _ => Language::BuildFile,
        }
    }

    pub fn fqn(&self) -> &str {
        &self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> NodeKind {
        self.kind.clone()
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.location.as_ref().map(|l| l.path.as_ref())
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
#[serde(tag = "command", rename_all = "snake_case")]
pub enum GraphQuery {
    /// List members or structure (Rich Listing)
    Ls {
        /// Target node FQN, defaults to project modules if null
        fqn: Option<String>,
        #[serde(default)]
        kind: Vec<NodeKind>,
        #[serde(default)]
        modifiers: Vec<String>,
    },

    /// Search for symbols
    Find {
        pattern: String,
        #[serde(default)]
        kind: Vec<NodeKind>,
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
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<QueryResultEdge>,
}

impl QueryResult {
    pub fn new(nodes: Vec<GraphNode>, edges: Vec<QueryResultEdge>) -> Self {
        Self { nodes, edges }
    }
}
