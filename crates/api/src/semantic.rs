use crate::ApiResult;
use crate::models::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, DisplayGraphNode, PositionContext,
    ReferenceQuery, SymbolLocation, SymbolQuery, SymbolResolution,
};
use async_trait::async_trait;

// ============================================================================
// Core Semantic Traits - Balanced Granularity
// ============================================================================

/// Symbol navigation: resolve, go-to-definition, go-to-type, go-to-implementation.
/// This is the most fundamental semantic capability for code navigation.
#[async_trait]
pub trait SymbolNavigator: Send + Sync {
    /// Resolve the symbol at a specific position in the source code.
    async fn resolve_symbol_at(&self, ctx: &PositionContext)
    -> ApiResult<Option<SymbolResolution>>;

    /// Find all definition locations for a given symbol query.
    async fn find_definitions(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>>;

    /// Find type definition locations (e.g., the class definition of a variable's type).
    async fn find_type_definitions(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>>;

    /// Find all implementation locations (e.g., classes implementing an interface).
    async fn find_implementations(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>>;

    /// Find occurrences of a symbol for document highlighting.
    async fn find_highlights(&self, ctx: &PositionContext) -> ApiResult<Vec<crate::models::Range>>;
}

/// Reference analysis: find all usages of a symbol.
#[async_trait]
pub trait ReferenceAnalyzer: Send + Sync {
    /// Find all reference locations for a given reference query.
    async fn find_references(&self, query: &ReferenceQuery) -> ApiResult<Vec<SymbolLocation>>;
}

/// Call hierarchy analysis: incoming and outgoing calls.
#[async_trait]
pub trait CallHierarchyAnalyzer: Send + Sync {
    /// Find all incoming calls (callers) to the specified function/method.
    async fn find_incoming_calls(&self, fqn: &str) -> ApiResult<Vec<CallHierarchyIncomingCall>>;

    /// Find all outgoing calls (callees) from the specified function/method.
    async fn find_outgoing_calls(&self, fqn: &str) -> ApiResult<Vec<CallHierarchyOutgoingCall>>;
}

/// Symbol metadata provider: detailed information about symbols.
#[async_trait]
pub trait SymbolInfoProvider: Send + Sync {
    /// Get detailed information about a symbol by its FQN.
    async fn get_symbol_info(&self, fqn: &str) -> ApiResult<Option<DisplayGraphNode>>;

    /// Get all symbols defined in a specific document.
    async fn get_document_symbols(&self, uri: &str) -> ApiResult<Vec<DisplayGraphNode>>;

    /// Get the language of a specific document.
    async fn get_language_for_document(
        &self,
        uri: &str,
    ) -> ApiResult<Option<crate::models::Language>>;
}
