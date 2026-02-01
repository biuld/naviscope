use crate::models::DisplayGraphNode;

/// Trait for language-specific feature providers.
/// This allows languages to provide rich information about their nodes
/// without the core or LSP layer needing to know about language-specific types.
pub trait LanguageFeatureProvider: Send + Sync {
    /// Render a human-readable detail view from a GraphNode's metadata.
    /// This is used for hover information, detailed views, etc.
    fn detail_view(&self, node: &DisplayGraphNode) -> Option<String>;

    /// Get a formatted signature for a node (e.g., method signature, field type).
    /// Returns None if the node kind doesn't have a meaningful signature.
    fn signature(&self, node: &DisplayGraphNode) -> Option<String>;

    /// Get formatted modifiers/attributes for a node.
    fn modifiers(&self, node: &DisplayGraphNode) -> Vec<String>;
}
