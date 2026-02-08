use crate::interner::StorageContext;
use crate::model::{BuildParseResult, GlobalParseResult};
use crate::naming::NamingConvention;
use crate::resolver::{BuildResolver, LangResolver, LspService, SemanticResolver};
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

    /// Get the type system for this language
    fn type_system(&self) -> Arc<dyn crate::type_system::TypeSystem>;

    /// Get the language-level resolver for graph builds
    fn lang_resolver(&self) -> Arc<dyn LangResolver>;

    /// Get the LSP parser for this language
    fn lsp_service(&self) -> Arc<dyn LspService>;

    /// Get the external resolver for classpath resolution (Phase 2+)
    fn external_resolver(&self) -> Option<Arc<dyn crate::resolver::ExternalResolver>> {
        None
    }

    /// Check if this plugin can handle an external asset (by extension) for stubbing.
    fn can_handle_external_asset(&self, _ext: &str) -> bool {
        false
    }

    /// Get the asset indexer for this language (Global Asset Scanner architecture)
    fn asset_indexer(&self) -> Option<Arc<dyn crate::AssetIndexer>> {
        None
    }

    /// Get the asset discoverer for this language (e.g., JdkDiscoverer for Java)
    fn global_asset_discoverer(&self) -> Option<Box<dyn crate::AssetDiscoverer>> {
        None
    }

    /// Get the asset source locator for this language (optional hook).
    fn asset_source_locator(&self) -> Option<Arc<dyn crate::AssetSourceLocator>> {
        None
    }

    /// Get the project-local asset discoverer for this language (optional hook).
    /// Use this for assets that exist only inside the current project (e.g. build outputs).
    fn project_asset_discoverer(
        &self,
        _project_root: &Path,
    ) -> Option<Box<dyn crate::AssetDiscoverer>> {
        None
    }
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

    /// Get the asset discoverer for this build tool (e.g., GradleCacheDiscoverer)
    fn asset_discoverer(&self) -> Option<Box<dyn crate::AssetDiscoverer>> {
        None
    }

    /// Get the asset source locator for this build tool (optional hook).
    fn asset_source_locator(&self) -> Option<Arc<dyn crate::AssetSourceLocator>> {
        None
    }

    /// Get the project-local asset discoverer for this build tool (optional hook).
    /// Use this for assets that exist only inside the current project (e.g. libs/*.jar).
    fn project_asset_discoverer(
        &self,
        _project_root: &Path,
    ) -> Option<Box<dyn crate::AssetDiscoverer>> {
        None
    }
}
