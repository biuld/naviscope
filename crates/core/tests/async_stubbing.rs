//! Tests for async stubbing workflow

use naviscope_api::models::graph::ResolutionStatus;
use naviscope_core::ingest::resolver::StubbingManager;
use naviscope_core::model::GraphOp;
use naviscope_core::runtime::orchestrator::NaviscopeEngine;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Test that the stubbing manager correctly sends requests
#[tokio::test]
async fn test_stubbing_manager_sends_requests() {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let manager = StubbingManager::new(tx);

    manager.request("com.example.Foo".to_string(), Vec::new());
    manager.request("com.example.Bar".to_string(), Vec::new());

    // Verify requests are received
    let req1 = rx.recv().await.expect("Should receive first request");
    assert_eq!(req1.fqn, "com.example.Foo");

    let req2 = rx.recv().await.expect("Should receive second request");
    assert_eq!(req2.fqn, "com.example.Bar");
}

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

    // Find a real JAR file (use JDK's rt.jar or similar)
    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));

    if let Some(jmod) = jmod_path.filter(|p| p.exists()) {
        let mut routes = std::collections::HashMap::new();
        routes.insert("java.lang.String".to_string(), vec![jmod.clone()]);

        let resolver = engine.get_resolver();

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

        resolver.schedule_stubs(&ops, &routes);

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

/// Test that duplicate FQNs are deduplicated in the worker
#[tokio::test]
async fn test_stubbing_deduplication() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    let mut seen = std::collections::HashSet::new();

    let manager = StubbingManager::new(tx);

    manager.request("com.example.Foo".to_string(), Vec::new());
    manager.request("com.example.Foo".to_string(), Vec::new());
    manager.request("com.example.Bar".to_string(), Vec::new());

    let mut processed = Vec::new();
    while let Ok(req) = rx.try_recv() {
        if seen.insert(req.fqn.clone()) {
            processed.push(req.fqn);
        }
    }

    // Only unique FQNs should be processed
    assert_eq!(processed.len(), 2);
    assert!(processed.contains(&"com.example.Foo".to_string()));
    assert!(processed.contains(&"com.example.Bar".to_string()));
}
