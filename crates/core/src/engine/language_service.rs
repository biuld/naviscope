//! Language service API for accessing language-specific features
//!
//! This trait provides a unified interface for accessing language-specific
//! functionality (parsers, resolvers, feature providers) without exposing
//! the underlying resolver implementation details.

use crate::parser::LspParser;
use crate::plugin::LanguageFeatureProvider;
use crate::project::source::Language;
use crate::resolver::SemanticResolver;
use std::path::Path;
use std::sync::Arc;

/// Unified API for accessing language-specific services
///
/// This trait abstracts away the resolver layer and provides a clean
/// interface for clients (LSP, CLI, MCP) to access language features.
pub trait LanguageService: Send + Sync {
    /// Get LSP parser for a specific language
    fn get_lsp_parser(&self, language: Language) -> Option<Arc<dyn LspParser>>;

    /// Get semantic resolver for a specific language
    fn get_semantic_resolver(&self, language: Language) -> Option<Arc<dyn SemanticResolver>>;

    /// Get language feature provider for a specific language
    fn get_feature_provider(&self, language: Language) -> Option<Arc<dyn LanguageFeatureProvider>>;

    /// Get language by file extension
    fn get_language_by_extension(&self, ext: &str) -> Option<Language>;

    /// Get parser and language for a file path (convenience method)
    ///
    /// This extracts the file extension from the path and returns
    /// both the parser and language if available.
    fn get_parser_and_lang_for_path(
        &self,
        path: &Path,
    ) -> Option<(Arc<dyn LspParser>, Language)> {
        let ext = path.extension()?.to_str()?;
        let lang = self.get_language_by_extension(ext)?;
        let parser = self.get_lsp_parser(lang)?;
        Some((parser, lang))
    }
}
