use crate::error::Result;
use crate::parser::GlobalParseResult;
use crate::resolver::SemanticResolver;
use std::path::Path;
use std::sync::Arc;

pub mod feature;
pub use feature::LanguageFeatureProvider;

/// Unified interface for language-specific support.
pub trait LanguagePlugin: Send + Sync {
    /// Plugin name, e.g., "java"
    fn name(&self) -> &str;

    /// Supported file extensions
    fn supported_extensions(&self) -> &[&str];

    /// Execute file parsing to extract nodes and relationships
    fn parse_file(&self, source: &str, path: &Path) -> Result<GlobalParseResult>;

    /// Get the semantic resolver for this language
    fn resolver(&self) -> Arc<dyn SemanticResolver>;

    /// Get the index resolver for this language
    fn lang_resolver(&self) -> Arc<dyn crate::resolver::LangResolver>;

    /// Get the LSP parser for this language
    fn lsp_parser(&self) -> Arc<dyn crate::parser::LspParser>;

    /// Get the feature provider for language-specific UI/LSP features
    fn feature_provider(&self) -> Arc<dyn LanguageFeatureProvider>;
}

/// Unified interface for build tool support.
pub trait BuildToolPlugin: Send + Sync {
    /// Plugin name, e.g., "gradle"
    fn name(&self) -> &str;

    /// Checks if this plugin can handle the given file name
    fn recognize(&self, file_name: &str) -> bool;

    /// Parse build-specific files
    fn parse_build_file(&self, source: &str) -> Result<BuildParseResult>;

    /// Get the build resolver
    fn build_resolver(&self) -> Arc<dyn crate::resolver::BuildResolver>;
}

/// Result of parsing a build file
pub struct BuildParseResult {
    // For now, mirroring what we have. Can be expanded.
    pub content: crate::project::scanner::ParsedContent,
}
