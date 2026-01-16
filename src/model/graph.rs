use super::psi::JavaElement;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub enum GraphNode {
    Code(JavaElement),
    Build(BuildElement),
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

#[derive(Serialize, Deserialize, Debug)]
pub enum BuildElement {
    Package(GradlePackage),
    Dependency(GradleDependency),
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GradlePackage {
    pub name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GradleDependency {
    pub group: String,
    pub name: String,
    pub version: String,
}
