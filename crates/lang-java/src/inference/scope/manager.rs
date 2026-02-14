//! Scope manager for handling variable scopes.

use super::table::SymbolTable;
use naviscope_api::models::TypeRef;
use std::collections::HashMap;

/// Identifier for a scope.
type ScopeId = usize;

/// Kind of scope
#[derive(Debug, Clone, PartialEq)]
pub enum ScopeKind {
    Local,
    Method,
    Class(String), // FQN
}

/// A scope containing symbols and a reference to its parent.
#[derive(Debug, Clone)]
pub struct Scope {
    pub id: ScopeId,
    pub parent_id: Option<ScopeId>,
    pub symbols: SymbolTable,
    pub kind: ScopeKind,
}

/// Manages scopes and symbol tables for a file.
#[derive(Debug, Default, Clone)]
pub struct ScopeManager {
    scopes: HashMap<ScopeId, Scope>,
    /// Map from AST node ID to the Scope ID it corresponds to.
    /// Only scope-creating nodes (e.g. Block, MethodDeclaration) are keys here.
    node_to_scope: HashMap<usize, ScopeId>,
}

impl ScopeManager {
    /// Create a new scope manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new scope for a given node.
    pub fn register_scope(
        &mut self,
        node_id: usize,
        parent_node_id: Option<usize>,
        kind: ScopeKind,
    ) -> ScopeId {
        let parent_scope_id = parent_node_id.and_then(|pid| self.node_to_scope.get(&pid).copied());

        // Use node_id as scope_id for simplicity, assuming they are unique from tree-sitter
        let scope_id = node_id;

        let scope = Scope {
            id: scope_id,
            parent_id: parent_scope_id,
            symbols: SymbolTable::new(),
            kind,
        };

        self.scopes.insert(scope_id, scope);
        self.node_to_scope.insert(node_id, scope_id);

        scope_id
    }

    /// Add a symbol to a scope.
    pub fn add_symbol(
        &mut self,
        scope_node_id: usize,
        name: String,
        ty: TypeRef,
        range: naviscope_api::models::symbol::Range,
    ) {
        if let Some(scope) = self.scopes.get_mut(&scope_node_id) {
            scope.symbols.insert(name, ty, range);
        }
    }

    /// Look up a symbol starting from a specific scope and walking up.
    /// Returns the normalized TypeRef.
    pub fn lookup(&self, start_scope_id: usize, name: &str) -> Option<TypeRef> {
        self.lookup_symbol(start_scope_id, name)
            .map(|si| si.type_ref)
    }

    /// Look up a symbol and its declaration range.
    pub fn lookup_symbol(
        &self,
        start_scope_id: usize,
        name: &str,
    ) -> Option<super::table::SymbolInfo> {
        let mut current_scope_id = Some(start_scope_id);

        while let Some(scope_id) = current_scope_id {
            if let Some(scope) = self.scopes.get(&scope_id) {
                if let Some(info) = scope.symbols.get(name) {
                    return Some(info.clone());
                }
                current_scope_id = scope.parent_id;
            } else {
                break;
            }
        }

        None
    }

    /// Find the enclosing class FQN for a given scope.
    pub fn find_enclosing_class(&self, start_scope_id: usize) -> Option<String> {
        let mut current_scope_id = Some(start_scope_id);

        while let Some(scope_id) = current_scope_id {
            if let Some(scope) = self.scopes.get(&scope_id) {
                if let ScopeKind::Class(fqn) = &scope.kind {
                    return Some(fqn.clone());
                }
                current_scope_id = scope.parent_id;
            } else {
                break;
            }
        }
        None
    }

    /// Get scope ID for a node if it exists
    pub fn get_scope_id(&self, node_id: usize) -> Option<ScopeId> {
        self.node_to_scope.get(&node_id).copied()
    }

    /// Get all class FQNs found in this file
    pub fn get_all_class_fqns(&self) -> Vec<String> {
        self.scopes
            .values()
            .filter_map(|s| {
                if let ScopeKind::Class(fqn) = &s.kind {
                    Some(fqn.clone())
                } else {
                    None
                }
            })
            .collect()
    }
}
