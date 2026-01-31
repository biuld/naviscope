use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::project::source::Language;
use smol_str::SmolStr;
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
    Custom(
        #[serde(with = "crate::util::serde_arc_str")]
        #[schemars(with = "String")]
        Arc<str>,
    ),
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
            _ => NodeKind::Custom(Arc::from(s)),
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

#[derive(Serialize, Deserialize, Debug, Clone, JsonSchema)]
pub struct GraphNode {
    // --- Identity ---
    #[serde(with = "crate::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub id: Arc<str>, // Unique Identifier (FQN)
    #[schemars(with = "String")]
    pub name: SmolStr, // Short display name
    pub kind: NodeKind, // Abstract categorization
    #[serde(with = "crate::util::serde_arc_str")]
    #[schemars(with = "String")]
    pub lang: Arc<str>, // Language identifier ("java", "rust", "buildfile")

    // --- Physical Location ---
    pub location: Option<NodeLocation>,

    // --- Extension Point ---
    #[serde(default = "empty_metadata")]
    pub metadata: serde_json::Value,
}

fn empty_metadata() -> serde_json::Value {
    serde_json::Value::Null
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct NodeLocation {
    #[serde(with = "crate::util::serde_arc_path")]
    #[schemars(with = "String")]
    pub path: Arc<Path>,
    pub range: Range,
    pub selection_range: Option<Range>, // Range of the identifier
}

impl GraphNode {
    pub fn language(&self) -> Language {
        match self.lang.as_ref() {
            "java" => Language::Java,
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

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GraphOp {
    /// Add or update a node
    AddNode {
        #[serde(with = "crate::util::serde_arc_str")]
        id: Arc<str>,
        data: GraphNode,
    },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        #[serde(with = "crate::util::serde_arc_str")]
        from_id: Arc<str>,
        #[serde(with = "crate::util::serde_arc_str")]
        to_id: Arc<str>,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath {
        #[serde(with = "crate::util::serde_arc_path")]
        path: Arc<Path>,
    },
    /// Update the reference index for a specific file
    UpdateIdentifiers {
        #[serde(with = "crate::util::serde_arc_path")]
        path: Arc<Path>,
        identifiers: Vec<SmolStr>,
    },
    /// Update file metadata (hash, mtime)
    UpdateFile {
        metadata: crate::project::source::SourceFile,
    },
}

/// Result of resolving a single file
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this file into the graph
    pub ops: Vec<GraphOp>,
    /// Fast access to nodes being added in this unit
    pub nodes: std::collections::HashMap<Arc<str>, GraphNode>,
    /// All unique identifier tokens in this file
    pub identifiers: Vec<SmolStr>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self {
            ops: Vec::new(),
            nodes: std::collections::HashMap::new(),
            identifiers: Vec::new(),
        }
    }

    pub fn add_node(&mut self, id: Arc<str>, data: GraphNode) {
        self.nodes.insert(id.clone(), data.clone());
        self.ops.push(GraphOp::AddNode { id, data });
    }

    pub fn add_edge(&mut self, from_id: Arc<str>, to_id: Arc<str>, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
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
