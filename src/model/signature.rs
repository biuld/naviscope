use serde::{Deserialize, Serialize};
use schemars::JsonSchema;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, Hash, JsonSchema)]
#[serde(tag = "kind", content = "data")]
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
