use crate::JavaPlugin;
use naviscope_plugin::LanguageParseCap;
use std::path::Path;

impl LanguageParseCap for JavaPlugin {
    fn parse_language_file(
        &self,
        source: &str,
        path: &Path,
    ) -> std::result::Result<naviscope_plugin::GlobalParseResult, naviscope_plugin::BoxError> {
        self.parser.parse_file(source, Some(path))
    }
}
