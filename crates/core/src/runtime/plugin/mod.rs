use crate::ingest::parser::{GlobalParseResult, LspParser};
use crate::ingest::resolver::SemanticResolver;
use crate::model::source::{BuildTool, Language};
use crate::model::{GraphNode, NodeMetadata};
use naviscope_api::models::DisplayGraphNode;
use std::path::Path;
use std::sync::Arc;

/// Interface for custom metadata encoding/decoding during storage.
pub trait MetadataPlugin: Send + Sync {
    /// Intern rich metadata into an optimized binary version for storage.
    fn intern(
        &self,
        _metadata: &dyn NodeMetadata,
        _ctx: &mut dyn crate::model::storage::model::StorageContext,
    ) -> Vec<u8> {
        // Default implementation could return empty bytes if no metadata is needed
        Vec::new()
    }

    /// Resolve optimized storage binary back into rich memory metadata.
    fn resolve(
        &self,
        bytes: &[u8],
        _ctx: &dyn crate::model::storage::model::StorageContext,
    ) -> Arc<dyn NodeMetadata>;
}

/// Interface for rendering internal nodes to display nodes.
pub trait NodeRenderer: Send + Sync {
    /// Convert internal GraphNode to DisplayGraphNode.
    fn render_display_node(&self, node: &GraphNode, rodeo: &dyn lasso::Reader) -> DisplayGraphNode;

    /// Hydrate an existing DisplayGraphNode with rich information.
    fn hydrate_display_node(&self, node: &mut DisplayGraphNode);
}

/// Unified interface for language-specific support.
pub trait LanguagePlugin: MetadataPlugin + NodeRenderer + Send + Sync {
    /// Plugin name, e.g., Language::JAVA
    fn name(&self) -> Language;

    /// Supported file extensions
    fn supported_extensions(&self) -> &[&str];

    /// Execute file parsing to extract nodes and relationships
    fn parse_file(&self, source: &str, path: &Path) -> crate::error::Result<GlobalParseResult>;

    /// Get the semantic resolver for this language
    fn resolver(&self) -> Arc<dyn SemanticResolver>;

    /// Get the index resolver for this language
    fn lang_resolver(&self) -> Arc<dyn crate::ingest::resolver::LangResolver>;

    /// Get the LSP parser for this language
    fn lsp_parser(&self) -> Arc<dyn LspParser>;
}

/// Unified interface for build tool support.
pub trait BuildToolPlugin: MetadataPlugin + NodeRenderer + Send + Sync {
    /// Plugin name, e.g., BuildTool::GRADLE
    fn name(&self) -> BuildTool;

    /// Checks if this plugin can handle the given file name
    fn recognize(&self, file_name: &str) -> bool;

    /// Parse build-specific files
    fn parse_build_file(&self, source: &str) -> crate::error::Result<BuildParseResult>;

    /// Get the build resolver
    fn build_resolver(&self) -> Arc<dyn crate::ingest::resolver::BuildResolver>;
}

/// Result of parsing a build file
pub struct BuildParseResult {
    // For now, mirroring what we have. Can be expanded.
    pub content: crate::ingest::scanner::ParsedContent,
}
