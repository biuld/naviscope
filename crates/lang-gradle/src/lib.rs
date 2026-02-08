pub mod discoverer;
pub mod model;
pub mod parser;
pub mod queries;
pub mod resolver;

pub use discoverer::GradleCacheDiscoverer;

use naviscope_api::models::BuildTool;
use naviscope_api::models::graph::DisplayGraphNode;
use naviscope_api::models::symbol::FqnReader;
use naviscope_plugin::{
    BuildContent, BuildParseResult, BuildToolPlugin, StandardNamingConvention, NamingConvention,
    NodeAdapter, PluginInstance, StorageContext,
};
use std::sync::Arc;

pub struct GradlePlugin {
    resolver: Arc<resolver::GradleResolver>,
}

impl NodeAdapter for GradlePlugin {
    fn render_display_node(
        &self,
        node: &naviscope_api::models::graph::GraphNode,
        fqns: &dyn FqnReader,
    ) -> DisplayGraphNode {
        let display_id = StandardNamingConvention.render_fqn(node.id, fqns);
        let mut display = DisplayGraphNode {
            id: display_id,
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: "gradle".to_string(),
            source: node.source.clone(),
            status: node.status,
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
        _ctx: &mut dyn StorageContext,
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
        _ctx: &dyn StorageContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::GradleNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(naviscope_api::models::graph::EmptyMetadata)
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

    fn parse_build_file(
        &self,
        source: &str,
    ) -> Result<BuildParseResult, Box<dyn std::error::Error + Send + Sync>> {
        if source.contains("include") && (source.contains("'") || source.contains("\"")) {
            let settings =
                parser::parse_settings(source).unwrap_or_else(|_| model::GradleSettings {
                    root_project_name: None,
                    included_projects: Vec::new(),
                });
            Ok(BuildParseResult {
                content: BuildContent::Metadata(
                    serde_json::to_value(settings).unwrap_or(serde_json::Value::Null),
                ),
            })
        } else {
            let deps = parser::parse_dependencies(source).unwrap_or_default();
            Ok(BuildParseResult {
                content: BuildContent::Metadata(
                    serde_json::to_value(model::GradleParseResult { dependencies: deps })
                        .unwrap_or(serde_json::Value::Null),
                ),
            })
        }
    }

    fn build_resolver(&self) -> Arc<dyn naviscope_plugin::BuildResolver> {
        self.resolver.clone()
    }

    fn asset_discoverer(&self) -> Option<Box<dyn naviscope_plugin::AssetDiscoverer>> {
        Some(Box::new(crate::discoverer::GradleCacheDiscoverer::new()))
    }
}
