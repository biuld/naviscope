use naviscope_api::models::NodeMetadata;
pub use naviscope_plugin::{IndexMetadata, SymbolInterner};
use std::sync::Arc;

/// Helper to implement IndexMetadata for metadata that is already in its final form.
pub fn identity_intern(metadata: Arc<dyn NodeMetadata>) -> Arc<dyn NodeMetadata> {
    metadata
}
