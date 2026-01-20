use super::lang::gradle::GradleElement;
use super::lang::java::JavaElement;
use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

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

    pub fn from_ts(range: tree_sitter::Range) -> Self {
        Self {
            start_line: range.start_point.row,
            start_col: range.start_point.column,
            end_line: range.end_point.row,
            end_col: range.end_point.column,
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
    pub fn fqn(&self) -> String {
        match self {
            GraphNode::Code(CodeElement::Java { element, .. }) => element.id().to_string(),
            GraphNode::Build(BuildElement::Gradle { element, .. }) => element.fqn(),
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
