//! Tests for async stubbing workflow

use naviscope_api::models::graph::ResolutionStatus;
use naviscope_core::indexing::stub_planner::StubPlanner;
use naviscope_core::model::GraphOp;
use naviscope_core::runtime::NaviscopeEngine;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

/// Test that JavaPlugin correctly reports external asset handling
#[test]
fn test_java_plugin_handles_external_assets() {
    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let generator = java_caps
        .asset
        .stub_generator()
        .expect("Java should provide a stub generator");

    // Java plugin should handle these
    assert!(generator.can_generate(Path::new("test.jar")));
    assert!(generator.can_generate(Path::new("java.base.jmod")));
    assert!(generator.can_generate(Path::new("modules")));

    // Java plugin should NOT handle these
    assert!(!generator.can_generate(Path::new("foo.rs")));
    assert!(!generator.can_generate(Path::new("foo.py")));
    assert!(!generator.can_generate(Path::new("foo.go")));
}

/// Test the full async stubbing flow with a real JAR file
#[tokio::test]
async fn test_async_stubbing_with_jar() {
    use std::time::Duration;

    // Create a temporary directory for the test project
    let temp_dir = std::env::temp_dir().join("naviscope_stub_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Create engine with Java plugin
    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine = NaviscopeEngine::builder(temp_dir.clone())
        .with_language_caps(java_caps)
        .build();
    let _ = engine.scan_global_assets().await;

    // Find a real JAR file (use JDK's rt.jar or similar)
    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));

    if let Some(jmod) = jmod_path.filter(|p| p.exists()) {
        let mut routes = std::collections::HashMap::new();
        routes.insert("java.lang.String".to_string(), vec![jmod.clone()]);

        let ops = vec![GraphOp::AddNode {
            data: Some(naviscope_plugin::IndexNode {
                id: naviscope_api::models::symbol::NodeId::Flat("java.lang.String".to_string()),
                name: "String".to_string(),
                kind: naviscope_api::models::graph::NodeKind::Class,
                lang: "java".to_string(),
                source: naviscope_api::models::graph::NodeSource::External,
                status: ResolutionStatus::Unresolved,
                location: None,
                metadata: Arc::new(naviscope_api::models::graph::EmptyMetadata),
            }),
        }];

        let reqs = StubPlanner::plan(&ops, &routes);
        for req in reqs {
            assert!(engine.request_stub_for_fqn(&req.fqn));
        }

        tokio::time::sleep(Duration::from_millis(500)).await;

        let graph = engine.snapshot().await;
        println!(
            "Graph after stubbing: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    } else {
        println!("Skipping JAR test: JAVA_HOME not set or jmods not found");
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}
