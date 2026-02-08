//! Symbol table for a single scope.

use naviscope_api::models::TypeRef;
use naviscope_api::models::symbol::Range;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub type_ref: TypeRef,
    pub range: Range,
}

/// A symbol table mapping variable names to their types and ranges.
#[derive(Debug, Default, Clone)]
pub struct SymbolTable {
    symbols: HashMap<String, SymbolInfo>,
}

impl SymbolTable {
    /// Create a new empty symbol table.
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a variable into the symbol table.
    pub fn insert(&mut self, name: String, ty: TypeRef, range: Range) {
        self.symbols.insert(
            name,
            SymbolInfo {
                type_ref: ty,
                range,
            },
        );
    }

    /// Look up a variable in this scope.
    pub fn get(&self, name: &str) -> Option<&SymbolInfo> {
        self.symbols.get(name)
    }
}
