use super::lang::gradle::GradleElement;
use super::lang::java::JavaElement;
use serde::{Deserialize, Serialize};
use clap::ValueEnum;
use schemars::JsonSchema;

use crate::project::source::Language;
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema, ValueEnum)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Class,
    Interface,
    Enum,
    Annotation,
    Method,
    Constructor,
    Field,
    Package,
    // Build specific
    Module,
    Dependency,
    Task,
    Plugin,
    // Fallback
    Other,
}

impl From<&str> for NodeKind {
    fn from(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "class" => NodeKind::Class,
            "interface" => NodeKind::Interface,
            "enum" => NodeKind::Enum,
            "annotation" => NodeKind::Annotation,
            "method" => NodeKind::Method,
            "constructor" => NodeKind::Constructor,
            "field" => NodeKind::Field,
            "package" => NodeKind::Package,
            "module" => NodeKind::Module,
            "dependency" => NodeKind::Dependency,
            "task" => NodeKind::Task,
            "plugin" => NodeKind::Plugin,
            _ => NodeKind::Other,
        }
    }
}

impl ToString for NodeKind {
    fn to_string(&self) -> String {
        match self {
            NodeKind::Class => "class".to_string(),
            NodeKind::Interface => "interface".to_string(),
            NodeKind::Enum => "enum".to_string(),
            NodeKind::Annotation => "annotation".to_string(),
            NodeKind::Method => "method".to_string(),
            NodeKind::Constructor => "constructor".to_string(),
            NodeKind::Field => "field".to_string(),
            NodeKind::Package => "package".to_string(),
            NodeKind::Module => "module".to_string(),
            NodeKind::Dependency => "dependency".to_string(),
            NodeKind::Task => "task".to_string(),
            NodeKind::Plugin => "plugin".to_string(),
            NodeKind::Other => "other".to_string(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GraphNode {
    Code(CodeElement),
    Build(BuildElement),
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum CodeElement {
    Java {
        element: JavaElement,
        file_path: Option<PathBuf>,
    },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum BuildElement {
    Gradle {
        element: GradleElement,
        file_path: Option<PathBuf>,
    },
}

impl GraphNode {
    pub fn language(&self) -> Language {
        match self {
            GraphNode::Code(CodeElement::Java { .. }) => Language::Java,
            GraphNode::Build(BuildElement::Gradle { .. }) => Language::BuildFile,
        }
    }

    pub fn fqn(&self) -> &str {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => element.id(),
            GraphNode::Build(BuildElement::Gradle { element, .. }) => element.id(),
        }
    }

    pub fn name(&self) -> &str {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => element.name(),
            GraphNode::Build(BuildElement::Gradle { element, .. }) => element.name(),
        }
    }

    pub fn kind(&self) -> NodeKind {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => match element {
                JavaElement::Class(_) => NodeKind::Class,
                JavaElement::Interface(_) => NodeKind::Interface,
                JavaElement::Enum(_) => NodeKind::Enum,
                JavaElement::Annotation(_) => NodeKind::Annotation,
                JavaElement::Method(m) => if m.is_constructor { NodeKind::Constructor } else { NodeKind::Method },
                JavaElement::Field(_) => NodeKind::Field,
                JavaElement::Package(_) => NodeKind::Package,
            },
            GraphNode::Build(BuildElement::Gradle { element, .. }) => NodeKind::from(element.kind()),
        }
    }

    pub fn file_path(&self) -> Option<&PathBuf> {
        match self {
            GraphNode::Code(CodeElement::Java { file_path, .. }) => file_path.as_ref(),
            GraphNode::Build(BuildElement::Gradle { file_path, .. }) => file_path.as_ref(),
        }
    }

    pub fn range(&self) -> Option<&Range> {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => element.range(),
            GraphNode::Build(_) => None,
        }
    }

    pub fn name_range(&self) -> Option<&Range> {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => element.name_range(),
            GraphNode::Build(_) => None,
        }
    }

    pub fn java(element: JavaElement, file_path: Option<PathBuf>) -> Self {
        GraphNode::Code(CodeElement::Java { element, file_path })
    }

    pub fn gradle(element: GradleElement, file_path: Option<PathBuf>) -> Self {
        GraphNode::Build(BuildElement::Gradle { element, file_path })
    }
}

/// Graph operation commands that can be computed in parallel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GraphOp {
    /// Add or update a node
    AddNode { id: String, data: GraphNode },
    /// Add an edge between two nodes (referenced by their IDs)
    AddEdge {
        from_id: String,
        to_id: String,
        edge: GraphEdge,
    },
    /// Remove all nodes and edges associated with a specific file path
    RemovePath { path: PathBuf },
}

/// Result of resolving a single file
#[derive(Debug)]
pub struct ResolvedUnit {
    /// The operations needed to integrate this file into the graph
    pub ops: Vec<GraphOp>,
}

impl ResolvedUnit {
    pub fn new() -> Self {
        Self { ops: Vec::new() }
    }

    pub fn add_node(&mut self, id: String, data: GraphNode) {
        self.ops.push(GraphOp::AddNode { id, data });
    }

    pub fn add_edge(&mut self, from_id: String, to_id: String, edge: GraphEdge) {
        self.ops.push(GraphOp::AddEdge {
            from_id,
            to_id,
            edge,
        });
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema, ValueEnum)]
pub enum EdgeType {
    // Structural relationships
    Contains,
    // Inheritance/Implementation
    InheritsFrom,
    Implements,
    // Usage/Reference
    Calls,
    Instantiates,
    TypedAs,
    DecoratedBy,
    // Build system relationships
    UsesDependency,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub struct GraphEdge {
    pub edge_type: EdgeType,
    pub range: Option<Range>,
}

impl GraphEdge {
    pub fn new(edge_type: EdgeType) -> Self {
        Self {
            edge_type,
            range: None,
        }
    }

    pub fn with_range(edge_type: EdgeType, range: Range) -> Self {
        Self {
            edge_type,
            range: Some(range),
        }
    }
}
