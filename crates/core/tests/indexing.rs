use naviscope_api::models::graph::{NodeKind, NodeSource, ResolutionStatus};
use naviscope_api::models::{BuildTool, EmptyMetadata, Range};
use naviscope_core::runtime::NaviscopeEngine;
use naviscope_plugin::{
    AssetCap, BuildCaps, BuildContent, BuildIndexCap, BuildParseCap, FileMatcherCap,
    MetadataCodecCap, ParsedFile, PresentationCap, ProjectContext, ResolvedUnit,
};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::Once;
use tempfile::tempdir;

struct MockBuildCap;

impl FileMatcherCap for MockBuildCap {
    fn supports_path(&self, path: &Path) -> bool {
        path.file_name().and_then(|n| n.to_str()) == Some("build.gradle")
    }
}

impl BuildParseCap for MockBuildCap {
    fn parse_build_file(
        &self,
        _source: &str,
    ) -> Result<naviscope_plugin::BuildParseResult, naviscope_plugin::BoxError> {
        Ok(naviscope_plugin::BuildParseResult {
            content: BuildContent::Unparsed(String::new()),
        })
    }
}

impl BuildIndexCap for MockBuildCap {
    fn compile_build(
        &self,
        files: &[&ParsedFile],
    ) -> Result<(ResolvedUnit, ProjectContext), naviscope_plugin::BoxError> {
        let mut unit = ResolvedUnit::new();
        let mut context = ProjectContext::new();
        if let Some(f) = files.first() {
            unit.add_node(naviscope_plugin::IndexNode {
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

impl AssetCap for MockBuildCap {}

impl PresentationCap for MockBuildCap {
    fn symbol_kind(&self, _kind: &NodeKind) -> lsp_types::SymbolKind {
        lsp_types::SymbolKind::MODULE
    }
}

impl MetadataCodecCap for MockBuildCap {}

fn mock_build_caps() -> BuildCaps {
    let cap = Arc::new(MockBuildCap);
    BuildCaps {
        build_tool: BuildTool::GRADLE,
        matcher: cap.clone(),
        parser: cap.clone(),
        indexing: cap.clone(),
        asset: cap.clone(),
        presentation: cap.clone(),
        metadata_codec: cap,
    }
}

fn ensure_test_index_dir() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join("naviscope_test_index_dir");
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("NAVISCOPE_INDEX_DIR", dir);
        }
    });
}

#[tokio::test]
async fn test_update_files_persistence_integration() {
    ensure_test_index_dir();
    let dir = tempdir().unwrap();
    let build_gradle = dir.path().join("build.gradle");
    fs::write(&build_gradle, "println 'hello'").unwrap();

    let engine = NaviscopeEngine::builder(dir.path().to_path_buf())
        .with_build_caps(mock_build_caps())
        .build();

    engine
        .update_files(vec![build_gradle.clone()])
        .await
        .unwrap();
    let graph = engine.snapshot().await;
    assert!(
        graph.node_count() > 0,
        "Project node should exist after first indexing"
    );

    engine.update_files(vec![build_gradle]).await.unwrap();
    let graph = engine.snapshot().await;
    assert!(
        graph.node_count() > 0,
        "Project node should still exist after re-indexing"
    );
}
