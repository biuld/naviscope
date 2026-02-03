use crate::interner::StorageContext;
use crate::model::{BuildParseResult, GlobalParseResult};
use crate::naming::NamingConvention;
use crate::resolver::{BuildResolver, LangResolver, LspParser, SemanticResolver};
use naviscope_api::models::graph::{DisplayGraphNode, GraphNode, NodeMetadata};
use naviscope_api::models::symbol::FqnReader;
use std::path::Path;
use std::sync::Arc;

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
    fn decode_metadata(&self, bytes: &[u8], ctx: &dyn StorageContext) -> Arc<dyn NodeMetadata>;
}

/// Unified interface for language-specific support.
pub trait LanguagePlugin: PluginInstance + Send + Sync {
    /// Plugin name, e.g., Language::JAVA
    fn name(&self) -> naviscope_api::models::Language;

    /// Supported file extensions
    fn supported_extensions(&self) -> &[&str];

    /// Execute file parsing to extract nodes and relationships
    fn parse_file(
        &self,
        source: &str,
        path: &Path,
    ) -> Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>>;

    /// Get the semantic resolver for this language
    fn resolver(&self) -> Arc<dyn SemanticResolver>;

    /// Get the language-level resolver for graph builds
    fn lang_resolver(&self) -> Arc<dyn LangResolver>;

    /// Get the LSP parser for this language
    fn lsp_parser(&self) -> Arc<dyn LspParser>;
}

/// Unified interface for build tool support.
pub trait BuildToolPlugin: PluginInstance + Send + Sync {
    /// Plugin name, e.g., BuildTool::GRADLE
    fn name(&self) -> naviscope_api::models::BuildTool;

    /// Checks if this plugin can handle the given file name
    fn recognize(&self, file_name: &str) -> bool;

    /// Parse build-specific files
    fn parse_build_file(
        &self,
        source: &str,
    ) -> Result<BuildParseResult, Box<dyn std::error::Error + Send + Sync>>;

    /// Get the build resolver
    fn build_resolver(&self) -> Arc<dyn BuildResolver>;
}
