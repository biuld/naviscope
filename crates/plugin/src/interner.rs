use naviscope_api::models::graph::NodeKind;
use naviscope_api::models::symbol::{FqnId, FqnReader, NodeId, Symbol};

/// Advanced interface for creating and managing structured identifiers.
/// Primarily used by plugins during indexing and core's graph construction.
pub trait FqnInterner: FqnReader {
    /// Intern a single string segment (atom).
    fn intern_atom(&self, name: &str) -> Symbol;

    /// Intern a structured node given its parent and local name.
    fn intern_node(&self, parent: Option<FqnId>, name: &str, kind: NodeKind) -> FqnId;

    /// Intern a complex NodeId (Flat or Structured).
    fn intern_node_id(&self, id: &NodeId) -> FqnId;
}

/// Context for metadata serialization/deserialization operations.
/// Provides access to shared string interners and other storage facilities.
pub trait StorageContext: Send + Sync {
    /// Get the string interner for converting strings to symbols.
    fn interner(&mut self) -> &mut dyn FqnInterner;

    /// Downcast to Any for plugin-specific context access.
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

/// Context for interning strings during metadata conversion.
pub trait SymbolInterner {
    fn intern_str(&mut self, s: &str) -> u32;
}
