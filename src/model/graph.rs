use super::lang::gradle::GradleElement;
use super::lang::java::JavaElement;
use serde::{Deserialize, Serialize};
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

    pub fn kind(&self) -> &str {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => match element {
                JavaElement::Class(_) => "class",
                JavaElement::Interface(_) => "interface",
                JavaElement::Enum(_) => "enum",
                JavaElement::Annotation(_) => "annotation",
                JavaElement::Method(_) => "method",
                JavaElement::Field(_) => "field",
            },
            GraphNode::Build(BuildElement::Gradle { element, .. }) => element.kind(),
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

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash, JsonSchema)]
pub enum EdgeType {
    // Structural relationships
    Contains,
    // Inheritance/Implementation
    InheritsFrom,
    Implements,
    // Usage/Reference
    Calls,
    References,
    Instantiates,
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
