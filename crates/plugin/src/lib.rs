use naviscope_api::models::graph::{DisplayGraphNode, GraphNode, NodeMetadata, StorageContext};
use naviscope_api::models::symbol::FqnReader;
use std::fmt::Debug;
use std::sync::Arc;

pub mod naming;
pub use naming::{DotPathConvention, NamingConvention};

/// Metadata for a plugin (plugin's own information).
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
}

/// Unified plugin handle according to V2 architecture.
pub struct PluginHandle {
    pub metadata: PluginInfo,
    pub instance: Arc<dyn PluginInstance>,
}

/// The core trait that all plugins must implement.
/// It uses "capability discovery" instead of "fat interface inheritance".
pub trait PluginInstance: Send + Sync {
    /// Get the naming convention for this plugin (if any).
    fn get_naming_convention(&self) -> Option<Arc<dyn NamingConvention>> {
        None
    }

    /// Get the node adapter for this plugin (if any).
    /// The node adapter handles both display rendering and metadata serialization.
    fn get_node_adapter(&self) -> Option<Arc<dyn NodeAdapter>> {
        None
    }
}

/// Unified interface for language-specific node processing.
/// Handles both display rendering and metadata serialization.
pub trait NodeAdapter: Send + Sync {
    // === Presentation Layer (Display) ===
    
    /// Convert internal GraphNode to DisplayGraphNode with full information.
    fn render_display_node(&self, node: &GraphNode, fqns: &dyn FqnReader) -> DisplayGraphNode;
    
    // === Storage Layer (Serialization) ===
    
    /// Serialize metadata for storage.
    fn encode_metadata(
        &self,
        _metadata: &dyn NodeMetadata,
        _ctx: &mut dyn StorageContext,
    ) -> Vec<u8> {
        // Default: no metadata to store
        Vec::new()
    }

    /// Deserialize metadata from storage.
    fn decode_metadata(
        &self,
        bytes: &[u8],
        ctx: &dyn StorageContext,
    ) -> Arc<dyn NodeMetadata>;
}
