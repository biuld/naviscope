//! Context for type inference.
//!
//! Holds the state passed through the inference strategy chain.

use crate::inference::core::type_system::JavaTypeSystem;
use crate::inference::core::types::TypeResolutionContext;
use crate::inference::scope::ScopeManager;
use naviscope_api::models::TypeRef;

/// Context for type inference
///
/// This is passed through the inference chain. It is immutable;
/// updates create new contexts.
#[derive(Clone)]
pub struct InferContext<'a> {
    /// Source code being analyzed
    pub source: &'a str,
    /// Type system for lookups
    pub ts: &'a dyn JavaTypeSystem,
    /// Current package
    pub package: Option<String>,
    /// Imports in the current file
    pub imports: Vec<String>,
    /// Enclosing class FQN (for `this` resolution)
    pub enclosing_class: Option<String>,
    /// Type parameters in scope
    pub type_parameters: Vec<String>,
    /// Expected type for bidirectional inference (check mode)
    pub expected_type: Option<TypeRef>,
    /// Optional Scope Manager for optimized lookup
    pub scope_manager: Option<&'a ScopeManager>,
    /// Types defined in the current file
    pub known_fqns: Vec<String>,
}

impl<'a> InferContext<'a> {
    /// Create a new inference context
    pub fn new(source: &'a str, ts: &'a dyn JavaTypeSystem) -> Self {
        Self {
            source,
            ts,
            package: None,
            imports: Vec::new(),
            enclosing_class: None,
            type_parameters: Vec::new(),
            expected_type: None,
            scope_manager: None,
            known_fqns: Vec::new(),
        }
    }

    /// Create a context with package and imports
    pub fn with_file_context(
        source: &'a str,
        ts: &'a dyn JavaTypeSystem,
        package: Option<String>,
        imports: Vec<String>,
    ) -> Self {
        Self {
            source,
            ts,
            package,
            imports,
            enclosing_class: None,
            type_parameters: Vec::new(),
            expected_type: None,
            scope_manager: None,
            known_fqns: Vec::new(),
        }
    }

    /// Set the enclosing class
    pub fn with_enclosing_class(mut self, class: String) -> Self {
        self.enclosing_class = Some(class);
        self
    }

    /// Set imports
    pub fn with_imports(mut self, imports: Vec<String>) -> Self {
        self.imports = imports;
        self
    }

    /// Set package
    pub fn with_package(mut self, package: Option<String>) -> Self {
        self.package = package;
        self
    }

    /// Set known FQNs
    pub fn with_known_fqns(mut self, fqns: Vec<String>) -> Self {
        self.known_fqns = fqns;
        self
    }

    /// Set expected type for checking mode
    pub fn with_expected_type(mut self, expected: Option<TypeRef>) -> Self {
        self.expected_type = expected;
        self
    }

    /// Set scope manager
    pub fn with_scope_manager(mut self, manager: &'a ScopeManager) -> Self {
        self.scope_manager = Some(manager);
        self
    }

    /// Convert to TypeResolutionContext
    pub fn to_resolution_context(&self) -> TypeResolutionContext {
        TypeResolutionContext {
            package: self.package.clone(),
            imports: self.imports.clone(),
            type_parameters: self.type_parameters.clone(),
            known_fqns: self.known_fqns.clone(),
        }
    }
}
