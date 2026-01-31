pub mod parser;
pub mod queries;
pub mod resolver;

use naviscope_core::error::Result;
use naviscope_core::plugin::{BuildParseResult, BuildToolPlugin};
use naviscope_core::resolver::BuildResolver;
use std::sync::Arc;

pub struct GradlePlugin {
    resolver: Arc<resolver::GradleResolver>,
}

impl GradlePlugin {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(resolver::GradleResolver::new()),
        }
    }
}

impl BuildToolPlugin for GradlePlugin {
    fn name(&self) -> &str {
        "gradle"
    }

    fn recognize(&self, file_name: &str) -> bool {
        file_name == "build.gradle"
            || file_name == "build.gradle.kts"
            || file_name == "settings.gradle"
            || file_name == "settings.gradle.kts"
    }

    fn parse_build_file(&self, source: &str) -> Result<BuildParseResult> {
        // This is a bit tricky because the original code had separate methods for build vs settings.
        // For now, let's keep it simple or just expose the resolver.
        // Actually, the plugin trait needs to be implemented.

        // This is a placeholder as the original scan_and_parse logic was hardcoded.
        // We might want to move that logic into the plugin.
        // For now, let's just return a dummy or implement basic dispatch.
        if source.contains("include") && (source.contains("'") || source.contains("\"")) {
            let settings = parser::parse_settings(source).unwrap_or_else(|_| {
                naviscope_core::model::lang::gradle::GradleSettings {
                    root_project_name: None,
                    included_projects: Vec::new(),
                }
            });
            Ok(BuildParseResult {
                content: naviscope_core::project::scanner::ParsedContent::GradleSettings(settings),
            })
        } else {
            let deps = parser::parse_dependencies(source).unwrap_or_default();
            Ok(BuildParseResult {
                content: naviscope_core::project::scanner::ParsedContent::Gradle(
                    naviscope_core::model::lang::gradle::GradleParseResult { dependencies: deps },
                ),
            })
        }
    }

    fn build_resolver(&self) -> Arc<dyn BuildResolver> {
        self.resolver.clone()
    }
}
