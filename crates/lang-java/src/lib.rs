pub mod feature;
pub mod model;
pub mod parser;
pub mod queries;
pub mod resolver;

use naviscope_core::error::Result;
use naviscope_core::parser::{GlobalParseResult, LspParser};
use naviscope_core::plugin::{LanguageFeatureProvider, LanguagePlugin};
use naviscope_core::resolver::SemanticResolver;
use std::path::Path;
use std::sync::Arc;

pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
    feature_provider: Arc<feature::JavaFeatureProvider>,
}

impl JavaPlugin {
    pub fn new() -> Result<Self> {
        let parser = Arc::new(parser::JavaParser::new()?);
        let resolver = Arc::new(resolver::JavaResolver {
            parser: (*parser).clone(),
        });
        let feature_provider = Arc::new(feature::JavaFeatureProvider::new());
        Ok(Self {
            parser,
            resolver,
            feature_provider,
        })
    }
}

impl LanguagePlugin for JavaPlugin {
    fn name(&self) -> &str {
        "java"
    }

    fn supported_extensions(&self) -> &[&str] {
        &["java"]
    }

    fn parse_file(&self, source: &str, path: &Path) -> Result<GlobalParseResult> {
        use naviscope_core::parser::IndexParser;
        self.parser.parse_file(source, Some(path))
    }

    fn resolver(&self) -> Arc<dyn SemanticResolver> {
        self.resolver.clone()
    }

    fn lang_resolver(&self) -> Arc<dyn naviscope_core::resolver::LangResolver> {
        self.resolver.clone()
    }

    fn lsp_parser(&self) -> Arc<dyn LspParser> {
        self.parser.clone()
    }

    fn feature_provider(&self) -> Arc<dyn LanguageFeatureProvider> {
        self.feature_provider.clone()
    }
}
