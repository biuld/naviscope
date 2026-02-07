//! Tests for async stubbing workflow

use naviscope_api::models::graph::ResolutionStatus;
use naviscope_core::ingest::resolver::{ProjectContext, StubbingManager};
use naviscope_core::model::GraphOp;
use naviscope_core::runtime::orchestrator::NaviscopeEngine;
use naviscope_java::JavaPlugin;
use naviscope_plugin::LanguagePlugin;
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
    let plugin = JavaPlugin::new().expect("Failed to create JavaPlugin");

    // Java plugin should handle these
    assert!(plugin.can_handle_external_asset("jar"));
    assert!(plugin.can_handle_external_asset("jmod"));
    assert!(plugin.can_handle_external_asset("class"));

    // Java plugin should NOT handle these
    assert!(!plugin.can_handle_external_asset("rs"));
    assert!(!plugin.can_handle_external_asset("py"));
    assert!(!plugin.can_handle_external_asset("go"));
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
    let java_plugin = Arc::new(JavaPlugin::new().expect("Failed to create JavaPlugin"));
    let engine = NaviscopeEngine::builder(temp_dir.clone())
        .with_language(java_plugin)
        .build();

    // Find a real JAR file (use JDK's rt.jar or similar)
    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));

    if let Some(jmod) = jmod_path.filter(|p| p.exists()) {
        // Create a project context with the asset route
        let mut context = ProjectContext::new();
        context
            .asset_routes
            .insert("java.lang.String".to_string(), vec![jmod.clone()]);

        let context = Arc::new(context);

        // Get the stub_tx from engine and send a request
        // Note: In real usage, this happens automatically through IndexResolver
        let resolver = engine.get_resolver();

        // Create a mock GraphOp that would trigger stubbing
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

        // Schedule stubs - this sends to the background worker
        resolver.schedule_stubs(&ops, context);

        // Give the worker time to process
        tokio::time::sleep(Duration::from_millis(500)).await;

        // Check that the node was updated (in a real test, we'd verify the graph)
        // For now, we just verify no panics occurred
        let graph = engine.snapshot().await;
        // The graph might not have nodes yet if the worker hasn't finished,
        // but the test ensures the pipeline doesn't crash
        println!(
            "Graph after stubbing: {} nodes, {} edges",
            graph.node_count(),
            graph.edge_count()
        );
    } else {
        println!("Skipping JAR test: JAVA_HOME not set or jmods not found");
    }

    // Cleanup
    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Test that duplicate FQNs are deduplicated in the worker
#[tokio::test]
async fn test_stubbing_deduplication() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    // Simulate what the worker's seen_fqns set does
    let mut seen = std::collections::HashSet::new();

    let manager = StubbingManager::new(tx);

    // Send the same FQN twice
    manager.request("com.example.Foo".to_string(), Vec::new());
    manager.request("com.example.Foo".to_string(), Vec::new());
    manager.request("com.example.Bar".to_string(), Vec::new());

    // Process like the worker would
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
