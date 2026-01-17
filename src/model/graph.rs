use super::lang::gradle::GradleElement;
use super::lang::java::JavaElement;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum GraphNode {
    Code(JavaElement),
    Build(GradleElement),
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
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
