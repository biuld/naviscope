use naviscope_api::models::graph::{NodeKind, NodeSource, ResolutionStatus};
use naviscope_api::models::{BuildTool, EmptyMetadata, Range};
use naviscope_core::ingest::parser::IndexNode;
use naviscope_core::ingest::resolver::BuildResolver;
use naviscope_core::runtime::orchestrator::NaviscopeEngine;
use naviscope_plugin::{
    BuildParseResult, BuildToolPlugin, NodeAdapter, ParsedFile, PluginInstance, ProjectContext,
    ResolvedUnit, StorageContext,
};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;

struct MockBuildPlugin;

impl PluginInstance for MockBuildPlugin {
    fn get_node_adapter(&self) -> Option<Arc<dyn NodeAdapter>> {
        Some(Arc::new(MockBuildPlugin))
    }
}

impl NodeAdapter for MockBuildPlugin {
    fn render_display_node(
        &self,
        _node: &naviscope_api::models::GraphNode,
        _rodeo: &dyn naviscope_api::models::symbol::FqnReader,
    ) -> naviscope_api::models::DisplayGraphNode {
        unimplemented!()
    }

    fn decode_metadata(
        &self,
        _bytes: &[u8],
        _ctx: &dyn StorageContext,
    ) -> Arc<dyn naviscope_api::models::NodeMetadata> {
        Arc::new(EmptyMetadata)
    }
}

impl BuildToolPlugin for MockBuildPlugin {
    fn name(&self) -> BuildTool {
        BuildTool::GRADLE
    }
    fn recognize(&self, name: &str) -> bool {
        name == "build.gradle"
    }
    fn parse_build_file(
        &self,
        _source: &str,
    ) -> Result<BuildParseResult, Box<dyn std::error::Error + Send + Sync>> {
        unimplemented!()
    }
    fn build_resolver(&self) -> Arc<dyn BuildResolver> {
        Arc::new(MockBuildResolver)
    }
}

struct MockBuildResolver;

impl BuildResolver for MockBuildResolver {
    fn resolve(
        &self,
        files: &[&ParsedFile],
    ) -> Result<(ResolvedUnit, ProjectContext), Box<dyn std::error::Error + Send + Sync>> {
        let mut unit = ResolvedUnit::new();
        let mut context = ProjectContext::new();
        if let Some(f) = files.first() {
            unit.add_node(IndexNode {
                id: naviscope_api::models::symbol::NodeId::Flat("project:test".to_string()),
                name: "test".to_string(),
                kind: NodeKind::Project,
                lang: "gradle".to_string(),
                source: NodeSource::Project,
                status: ResolutionStatus::Resolved,
                location: Some(naviscope_api::models::DisplaySymbolLocation {
                    path: f.path().to_string_lossy().to_string(),
                    range: Range::default(),
                    selection_range: None,
                }),
                metadata: Arc::new(EmptyMetadata),
            });
            context.path_to_module.insert(
                f.path().parent().unwrap().to_path_buf(),
                "project:test".to_string(),
            );
        }
        Ok((unit, context))
    }
}

#[tokio::test]
async fn test_update_files_persistence_integration() {
    let dir = tempdir().unwrap();
    let build_gradle = dir.path().join("build.gradle");
    fs::write(&build_gradle, "println 'hello'").unwrap();

    let mut engine = NaviscopeEngine::new(dir.path().to_path_buf());
    engine.register_build_tool(Arc::new(MockBuildPlugin));

    // First index
    engine
        .update_files(vec![build_gradle.clone()])
        .await
        .unwrap();

    let graph = engine.snapshot().await;

    // Use a more direct check on nodes
    let has_project = graph.node_count() > 0;
    assert!(
        has_project,
        "Project node should exist after first indexing"
    );

    // Second index (incremental update on the same file)
    engine.update_files(vec![build_gradle]).await.unwrap();

    let graph = engine.snapshot().await;
    assert!(
        graph.node_count() > 0,
        "Project node should still exist after re-indexing"
    );
}
