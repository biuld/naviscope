use crate::GradlePlugin;
use naviscope_plugin::{BuildContent, BuildParseCap, BuildParseResult};

impl BuildParseCap for GradlePlugin {
    fn parse_build_file(
        &self,
        source: &str,
    ) -> Result<BuildParseResult, Box<dyn std::error::Error + Send + Sync>> {
        if source.contains("include") && (source.contains("'") || source.contains("\"")) {
            let settings =
                crate::parser::parse_settings(source).unwrap_or_else(|_| crate::model::GradleSettings {
                    root_project_name: None,
                    included_projects: Vec::new(),
                });
            Ok(BuildParseResult {
                content: BuildContent::Metadata(
                    serde_json::to_value(settings).unwrap_or(serde_json::Value::Null),
                ),
            })
        } else {
            let deps = crate::parser::parse_dependencies(source).unwrap_or_default();
            Ok(BuildParseResult {
                content: BuildContent::Metadata(
                    serde_json::to_value(crate::model::GradleParseResult { dependencies: deps })
                        .unwrap_or(serde_json::Value::Null),
                ),
            })
        }
    }
}
