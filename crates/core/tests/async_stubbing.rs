//! Tests for async stubbing workflow

use naviscope_api::models::graph::ResolutionStatus;
use naviscope_api::graph::GraphService;
use naviscope_api::models::{PositionContext, SymbolResolution};
use naviscope_api::semantic::SymbolInfoProvider;
use naviscope_api::semantic::SymbolNavigator;
use naviscope_core::indexing::source::plan_stub_requests;
use naviscope_core::facade::EngineHandle;
use naviscope_core::model::GraphOp;
use naviscope_core::runtime::NaviscopeEngine;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Once;

fn ensure_test_index_dir() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join("naviscope_test_index_dir_async_stubbing");
        std::fs::create_dir_all(&dir).unwrap();
        unsafe {
            std::env::set_var("NAVISCOPE_INDEX_DIR", dir);
        }
    });
}

fn offset_to_point(content: &str, offset: usize) -> (usize, usize) {
    let mut line = 0usize;
    let mut col = 0usize;
    for (i, ch) in content.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += ch.len_utf16();
        }
    }
    (line, col)
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
    ensure_test_index_dir();

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

        let reqs = plan_stub_requests(&ops, &routes);
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

/// Ensure realtime stub requests can materialize external symbols once source runtime is started.
#[tokio::test]
async fn test_realtime_stub_hydration_after_runtime_started() {
    use std::time::Duration;
    ensure_test_index_dir();

    let temp_dir = std::env::temp_dir().join("naviscope_stub_runtime_started");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));
    let Some(_jmod) = jmod_path.filter(|p| p.exists()) else {
        println!("Skipping realtime stub hydration test: JAVA_HOME not set or jmods not found");
        let _ = std::fs::remove_dir_all(&temp_dir);
        return;
    };

    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine = Arc::new(
        NaviscopeEngine::builder(temp_dir.clone())
            .with_language_caps(java_caps)
            .build(),
    );

    // Build+source update starts source runtime lazily via submit_source_stream.
    let java_file = temp_dir.join("Bootstrap.java");
    std::fs::write(&java_file, "class Bootstrap {}").unwrap();
    engine.update_files(vec![java_file]).await.unwrap();
    let _ = engine.scan_global_assets().await;

    assert!(engine.request_stub_for_fqn("java.lang.String"));

    let handle = EngineHandle::from_engine(Arc::clone(&engine));
    let mut found = None;
    for _ in 0..20 {
        if let Ok(Some(node)) = handle.get_node_display("java.lang.String").await {
            found = Some(node);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let node = found.expect("expected java.lang.String to be hydrated from stub request");
    assert_eq!(node.source, naviscope_api::models::NodeSource::External);

    // Also verify SymbolInfoProvider path (used by hover chain) can resolve hydrated stub symbol.
    let symbol_info = handle
        .get_symbol_info("java.lang.String")
        .await
        .expect("get_symbol_info should not fail")
        .expect("java.lang.String should exist after stub hydration");
    assert_eq!(symbol_info.source, naviscope_api::models::NodeSource::External);
    assert_eq!(symbol_info.id, "java.lang.String");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Verify overload resolution remains precise for external/stub symbols.
#[tokio::test]
async fn test_external_stub_overload_resolution_is_precise() {
    use std::time::Duration;
    ensure_test_index_dir();

    let temp_dir = std::env::temp_dir().join("naviscope_stub_overload_precise");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));
    let Some(_jmod) = jmod_path.filter(|p| p.exists()) else {
        println!("Skipping external overload test: JAVA_HOME not set or jmods not found");
        let _ = std::fs::remove_dir_all(&temp_dir);
        return;
    };

    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine = Arc::new(
        NaviscopeEngine::builder(temp_dir.clone())
            .with_language_caps(java_caps)
            .build(),
    );

    let file = temp_dir.join("Use.java");
    let source = r#"
class Use {
    void run() {
        "abc".indexOf(97);
        "abc".indexOf("a");
    }
}
"#;
    std::fs::write(&file, source).unwrap();

    // Start source runtime and index the source file.
    engine.update_files(vec![file.clone()]).await.unwrap();
    let _ = engine.scan_global_assets().await;
    assert!(engine.request_stub_for_fqn("java.lang.String"));

    let handle = EngineHandle::from_engine(Arc::clone(&engine));

    // Ensure base class stub is hydrated first.
    let mut string_ready = false;
    for _ in 0..20 {
        if let Ok(Some(node)) = handle.get_node_display("java.lang.String").await
            && node.source == naviscope_api::models::NodeSource::External
        {
            string_ready = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(string_ready, "java.lang.String should be hydrated before overload checks");

    let call_int = source.find("indexOf(97)").expect("indexOf(97) should exist");
    let (line_int, col_int) = offset_to_point(source, call_int);
    let call_str = source
        .find("indexOf(\"a\")")
        .expect("indexOf(\"a\") should exist");
    let (line_str, col_str) = offset_to_point(source, call_str);

    let mut maybe_int = None;
    let mut maybe_bool = None;
    for _ in 0..30 {
        maybe_int = handle
            .resolve_symbol_at(&PositionContext {
                uri: format!("file://{}", file.display()),
                line: line_int as u32,
                char: col_int as u32,
                content: Some(source.to_string()),
            })
            .await
            .ok()
            .flatten();
        maybe_bool = handle
            .resolve_symbol_at(&PositionContext {
                uri: format!("file://{}", file.display()),
                line: line_str as u32,
                char: col_str as u32,
                content: Some(source.to_string()),
            })
            .await
            .ok()
            .flatten();

        if maybe_int.is_some() && maybe_bool.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let res_int = maybe_int.expect("int call should resolve");
    let res_bool = maybe_bool.expect("bool call should resolve");

    let fqn_int = match res_int {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => fqn,
        SymbolResolution::Local(_, _) => panic!("expected non-local resolution for int call"),
    };
    let fqn_bool = match res_bool {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => fqn,
        SymbolResolution::Local(_, _) => panic!("expected non-local resolution for string call"),
    };

    assert_ne!(
        fqn_int, fqn_bool,
        "different indexOf overload callsites should resolve to different FQNs"
    );
    assert!(
        fqn_int.starts_with("java.lang.String#indexOf(")
            && fqn_bool.starts_with("java.lang.String#indexOf("),
        "both resolutions should map to String.indexOf overloads"
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
}

/// Ensure stub requests queued before source runtime startup are replayed after runtime starts.
#[tokio::test]
async fn test_stub_request_replayed_after_runtime_start() {
    use std::time::Duration;
    ensure_test_index_dir();

    let temp_dir = std::env::temp_dir().join("naviscope_stub_replay_after_start");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let java_home = std::env::var("JAVA_HOME").ok();
    let jmod_path = java_home
        .as_ref()
        .map(|h| PathBuf::from(h).join("jmods").join("java.base.jmod"));
    let Some(_jmod) = jmod_path.filter(|p| p.exists()) else {
        println!("Skipping pending-stub replay test: JAVA_HOME not set or jmods not found");
        let _ = std::fs::remove_dir_all(&temp_dir);
        return;
    };

    let java_caps = naviscope_java::java_caps().expect("Failed to create Java caps");
    let engine = Arc::new(
        NaviscopeEngine::builder(temp_dir.clone())
            .with_language_caps(java_caps)
            .build(),
    );

    // Build asset routes first; runtime is still not started.
    let _ = engine.scan_global_assets().await;
    assert!(
        engine.request_stub_for_fqn("java.lang.String"),
        "request should be accepted and queued before runtime start"
    );

    let handle = EngineHandle::from_engine(Arc::clone(&engine));
    let before_start = handle.get_node_display("java.lang.String").await.unwrap();
    assert!(
        before_start.is_none(),
        "queued request should not materialize symbol before runtime start"
    );

    // Trigger runtime startup through source update.
    let java_file = temp_dir.join("Boot.java");
    std::fs::write(&java_file, "class Boot {}").unwrap();
    engine.update_files(vec![java_file]).await.unwrap();

    let mut found = None;
    for _ in 0..20 {
        if let Ok(Some(node)) = handle.get_node_display("java.lang.String").await {
            found = Some(node);
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    let node = found.expect("queued stub request should be replayed after runtime starts");
    assert_eq!(node.source, naviscope_api::models::NodeSource::External);

    let _ = std::fs::remove_dir_all(&temp_dir);
}
