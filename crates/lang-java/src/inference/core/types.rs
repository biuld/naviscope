//! Data structures for the type inference system.
//!
//! These are pure data types with no behavior logic.

use naviscope_api::models::TypeRef;

/// Helper trait for TypeRef operations
pub trait TypeRefExt {
    fn as_fqn(&self) -> Option<String>;
}

impl TypeRefExt for TypeRef {
    fn as_fqn(&self) -> Option<String> {
        match self {
            TypeRef::Id(fqn) => Some(fqn.clone()),
            TypeRef::Generic { base, .. } => base.as_fqn(),
            _ => None,
        }
    }
}

/// Information about a type (class, interface, enum, etc.)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    /// Fully qualified name, e.g., "java.util.List"
    pub fqn: String,
    /// Kind of type
    pub kind: TypeKind,
    /// Modifiers like public, abstract, final
    pub modifiers: Vec<String>,
    /// Generic type parameters, e.g., `<T, U extends Comparable<T>>`
    pub type_parameters: Vec<TypeParameter>,
}

/// Kind of type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeKind {
    Class,
    Interface,
    Enum,
    Annotation,
    Primitive,
}

/// A generic type parameter declaration
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeParameter {
    /// Parameter name, e.g., "T"
    pub name: String,
    /// Upper bounds, e.g., ["Comparable", "Serializable"] for `T extends Comparable & Serializable`
    pub bounds: Vec<String>,
}

/// Information about a member (field, method, constructor)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberInfo {
    /// Simple name, e.g., "get" or "size"
    pub name: String,
    /// Fully qualified name, e.g., "java.util.List#get"
    pub fqn: String,
    /// Kind of member
    pub kind: MemberKind,
    /// The type that declares this member (may differ from lookup type due to inheritance)
    pub declaring_type: String,
    /// Field type or method return type
    pub type_ref: TypeRef,
    /// Method parameters (None for fields)
    pub parameters: Option<Vec<ParameterInfo>>,
    /// Modifiers like public, static, final
    pub modifiers: Vec<String>,
    /// Raw generic signature from bytecode, if available
    pub generic_signature: Option<String>,
}

/// Kind of member
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberKind {
    Field,
    Method,
    Constructor,
}

/// Information about a method parameter
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParameterInfo {
    /// Parameter name (may be synthetic like "arg0")
    pub name: String,
    /// Parameter type
    pub type_ref: TypeRef,
    /// True when this parameter is declared with `...` varargs syntax.
    pub is_varargs: bool,
}

/// Context for type name resolution
#[derive(Debug, Clone, Default)]
pub struct TypeResolutionContext {
    /// Current package, e.g., "com.example"
    pub package: Option<String>,
    /// Import statements in the file
    pub imports: Vec<String>,
    /// Type parameters in scope (for generic methods/classes)
    pub type_parameters: Vec<String>,
    /// Types defined in the current file (FQN list)
    pub known_fqns: Vec<String>,
}
