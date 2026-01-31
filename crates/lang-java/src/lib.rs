pub mod parser;
pub mod queries;
pub mod resolver;

use naviscope_core::error::Result;
use naviscope_core::parser::{GlobalParseResult, LspParser};
use naviscope_core::plugin::LanguagePlugin;
use naviscope_core::resolver::SemanticResolver;
use std::path::Path;
use std::sync::Arc;

pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
}

impl JavaPlugin {
    pub fn new() -> Result<Self> {
        let parser = Arc::new(parser::JavaParser::new()?);
        let resolver = Arc::new(resolver::JavaResolver {
            parser: (*parser).clone(),
        });
        Ok(Self { parser, resolver })
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
}
