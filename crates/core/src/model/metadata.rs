use naviscope_api::models::NodeMetadata;
use std::sync::Arc;

/// Context for interning strings during metadata conversion.
pub trait SymbolInterner {
    fn intern_str(&mut self, s: &str) -> u32;
}

/// Compilation-time/Index-time metadata.
/// This version usually contains strings and is used during the parsing phase.
/// It must be able to convert itself into a runtime NodeMetadata.
pub trait IndexMetadata: Send + Sync + std::fmt::Debug {
    /// Cast to Any for downcasting to concrete types.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Transform this metadata into its interned/optimized version for graph storage.
    fn intern(&self, interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata>;
}

impl IndexMetadata for naviscope_api::models::EmptyMetadata {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn intern(&self, _interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata> {
        Arc::new(self.clone())
    }
}

/// Helper to implement IndexMetadata for metadata that is already in its final form.
pub fn identity_intern(metadata: Arc<dyn NodeMetadata>) -> Arc<dyn NodeMetadata> {
    metadata
}
