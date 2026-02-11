use crate::asset::BoxError;
use crate::model::{BuildParseResult, GlobalParseResult};
use std::path::Path;

pub trait LanguageParseCap: Send + Sync {
    fn parse_language_file(&self, source: &str, path: &Path) -> Result<GlobalParseResult, BoxError>;
}

pub trait BuildParseCap: Send + Sync {
    fn parse_build_file(&self, source: &str) -> Result<BuildParseResult, BoxError>;
}
