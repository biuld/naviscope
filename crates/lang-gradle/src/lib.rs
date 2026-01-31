pub mod feature;
pub mod model;
pub mod parser;
pub mod queries;
pub mod resolver;

use naviscope_core::error::Result;
use naviscope_core::plugin::{BuildParseResult, BuildToolPlugin, MetadataPlugin};
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

impl MetadataPlugin for GradlePlugin {
    fn intern(
        &self,
        value: serde_json::Value,
        ctx: &mut dyn naviscope_core::engine::storage::model::StorageContext,
    ) -> serde_json::Value {
        if let Ok(element) = serde_json::from_value::<crate::model::GradleElement>(value) {
            let storage_element = element.intern(ctx);
            serde_json::to_value(&storage_element).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
        }
    }

    fn resolve(
        &self,
        value: serde_json::Value,
        ctx: &dyn naviscope_core::engine::storage::model::StorageContext,
    ) -> serde_json::Value {
        if let Ok(storage_element) = serde_json::from_value::<crate::model::GradleStorageElement>(value)
        {
            let element = storage_element.resolve(ctx);
            serde_json::to_value(element).unwrap_or(serde_json::Value::Null)
        } else {
            serde_json::Value::Null
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
            let settings =
                parser::parse_settings(source).unwrap_or_else(|_| model::GradleSettings {
                    root_project_name: None,
                    included_projects: Vec::new(),
                });
            Ok(BuildParseResult {
                content: naviscope_core::project::scanner::ParsedContent::MetaData(
                    serde_json::to_value(settings).unwrap_or(serde_json::Value::Null),
                ),
            })
        } else {
            let deps = parser::parse_dependencies(source).unwrap_or_default();
            Ok(BuildParseResult {
                content: naviscope_core::project::scanner::ParsedContent::MetaData(
                    serde_json::to_value(model::GradleParseResult { dependencies: deps })
                        .unwrap_or(serde_json::Value::Null),
                ),
            })
        }
    }

    fn build_resolver(&self) -> Arc<dyn BuildResolver> {
        self.resolver.clone()
    }
}
