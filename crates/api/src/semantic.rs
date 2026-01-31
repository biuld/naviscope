use crate::models::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, PositionContext, ReferenceQuery,
    SymbolInfo, SymbolLocation, SymbolQuery, SymbolResolution,
};
use async_trait::async_trait;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum SemanticError {
    #[error("Symbol not found: {0}")]
    SymbolNotFound(String),
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("Language not supported: {0}")]
    UnsupportedLanguage(String),
}

pub type Result<T> = std::result::Result<T, SemanticError>;

// ============================================================================
// Core Semantic Traits - Balanced Granularity
// ============================================================================

/// Symbol navigation: resolve, go-to-definition, go-to-type, go-to-implementation.
/// This is the most fundamental semantic capability for code navigation.
#[async_trait]
pub trait SymbolNavigator: Send + Sync {
    /// Resolve the symbol at a specific position in the source code.
    async fn resolve_symbol_at(&self, ctx: &PositionContext) -> Result<Option<SymbolResolution>>;

    /// Find all definition locations for a given symbol query.
    async fn find_definitions(&self, query: &SymbolQuery) -> Result<Vec<SymbolLocation>>;

    /// Find type definition locations (e.g., the class definition of a variable's type).
    async fn find_type_definitions(&self, query: &SymbolQuery) -> Result<Vec<SymbolLocation>>;

    /// Find all implementation locations (e.g., classes implementing an interface).
    async fn find_implementations(&self, query: &SymbolQuery) -> Result<Vec<SymbolLocation>>;

    /// Find occurrences of a symbol for document highlighting.
    async fn find_highlights(&self, ctx: &PositionContext) -> Result<Vec<crate::models::Range>>;
}

/// Reference analysis: find all usages of a symbol.
#[async_trait]
pub trait ReferenceAnalyzer: Send + Sync {
    /// Find all reference locations for a given reference query.
    async fn find_references(&self, query: &ReferenceQuery) -> Result<Vec<SymbolLocation>>;
}

/// Call hierarchy analysis: incoming and outgoing calls.
#[async_trait]
pub trait CallHierarchyAnalyzer: Send + Sync {
    /// Find all incoming calls (callers) to the specified function/method.
    async fn find_incoming_calls(&self, fqn: &str) -> Result<Vec<CallHierarchyIncomingCall>>;

    /// Find all outgoing calls (callees) from the specified function/method.
    async fn find_outgoing_calls(&self, fqn: &str) -> Result<Vec<CallHierarchyOutgoingCall>>;
}

/// Symbol metadata provider: detailed information about symbols.
#[async_trait]
pub trait SymbolInfoProvider: Send + Sync {
    /// Get detailed information about a symbol by its FQN.
    async fn get_symbol_info(&self, fqn: &str) -> Result<Option<SymbolInfo>>;

    /// Get all symbols defined in a specific document.
    async fn get_document_symbols(&self, uri: &str) -> Result<Vec<crate::models::DocumentSymbol>>;

    /// Get the language of a specific document.
    async fn get_language_for_document(&self, uri: &str)
    -> Result<Option<crate::models::Language>>;
}
