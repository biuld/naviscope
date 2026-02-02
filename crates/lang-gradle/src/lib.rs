pub mod model;
pub mod parser;
pub mod queries;
pub mod resolver;

use naviscope_api::models::DisplayGraphNode;
use naviscope_api::models::symbol::FqnReader;
use naviscope_core::error::Result;
use naviscope_core::ingest::resolver::BuildResolver;
use naviscope_core::ingest::scanner::ParsedContent;
use naviscope_core::model::source::BuildTool;
use naviscope_core::plugin::{
    BuildParseResult, BuildToolPlugin, NamingConvention, NodeAdapter, PluginInstance,
};
use std::sync::Arc;

pub struct GradlePlugin {
    resolver: Arc<resolver::GradleResolver>,
}

impl NodeAdapter for GradlePlugin {
    fn render_display_node(
        &self,
        node: &naviscope_core::model::GraphNode,
        fqns: &dyn FqnReader,
    ) -> DisplayGraphNode {
        let display_id = naviscope_plugin::DotPathConvention.render_fqn(node.id, fqns);
        let mut display = DisplayGraphNode {
            id: display_id,
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: "gradle".to_string(),
            location: node.location.as_ref().map(|l| l.to_display(fqns)),
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        };

        if let Some(gradle_meta) = node
            .metadata
            .as_any()
            .downcast_ref::<crate::model::GradleNodeMetadata>()
        {
            display.detail = gradle_meta.detail_view(fqns);
        }

        display
    }

    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn naviscope_api::models::graph::StorageContext,
    ) -> Vec<u8> {
        if let Some(gradle_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::GradleNodeMetadata>()
        {
            rmp_serde::to_vec(&gradle_meta).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn decode_metadata(
        &self,
        bytes: &[u8],
        _ctx: &dyn naviscope_api::models::graph::StorageContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::GradleNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(naviscope_core::model::EmptyMetadata)
        }
    }
}

impl GradlePlugin {
    pub fn new() -> Self {
        Self {
            resolver: Arc::new(resolver::GradleResolver::new()),
        }
    }
}

impl PluginInstance for GradlePlugin {
    fn get_node_adapter(&self) -> Option<Arc<dyn NodeAdapter>> {
        Some(Arc::new(Self::new()))
    }
}

impl BuildToolPlugin for GradlePlugin {
    fn name(&self) -> BuildTool {
        BuildTool::GRADLE
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
                content: ParsedContent::MetaData(
                    serde_json::to_value(settings).unwrap_or(serde_json::Value::Null),
                ),
            })
        } else {
            let deps = parser::parse_dependencies(source).unwrap_or_default();
            Ok(BuildParseResult {
                content: ParsedContent::MetaData(
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
