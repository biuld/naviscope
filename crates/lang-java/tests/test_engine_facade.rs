mod common;

use common::{offset_to_point, setup_java_engine};
use naviscope_api::models::{PositionContext, ReferenceQuery, SymbolQuery, SymbolResolution};
use naviscope_api::semantic::{CallHierarchyAnalyzer, ReferenceAnalyzer, SymbolNavigator};

#[tokio::test]
async fn test_full_engine_java_facade() {
    let temp_dir = std::env::temp_dir().join("naviscope_java_facade_test");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    let files = vec![
        (
            "com/example/Base.java",
            "package com.example; public interface Base { void run(); }",
        ),
        (
            "com/example/Impl.java",
            "package com.example; public class Impl implements Base { public void run() {} }",
        ),
        (
            "com/example/App.java",
            r#"
package com.example;
public class App {
    void start() {
        Base b = new Impl();
        b.run();
    }
}
"#,
        ),
    ];

    let handle = setup_java_engine(&temp_dir, files).await;
    let graph = handle.graph().await;

    // Demonstrate registering a convention (even if default is already Standard)
    // This verifies the API is accessible and working.
    graph.register_naming_convention(Box::new(
        naviscope_plugin::StandardNamingConvention::default(),
    ));

    graph.topology(); // Ensure graph is usable

    // 1. Resolve 'run' call in App.java (line 6 roughly)
    let app_path = temp_dir.join("com/example/App.java");
    let app_content = std::fs::read_to_string(&app_path).unwrap();
    let run_pos = app_content.find("b.run()").unwrap() + 2; // Point to 'run'
    let (line, col) = offset_to_point(&app_content, run_pos);

    let ctx = PositionContext {
        uri: format!("file://{}", app_path.display()),
        line: line as u32,
        char: col as u32,
        content: Some(app_content.clone()),
    };

    let resolution = handle
        .resolve_symbol_at(&ctx)
        .await
        .unwrap()
        .expect("Should resolve b.run()");

    // Should resolve to com.example.Base#run
    match &resolution {
        SymbolResolution::Precise(fqn, _) => assert_eq!(fqn, "com.example.Base#run"),
        SymbolResolution::Global(fqn) => assert_eq!(fqn, "com.example.Base#run"),
        _ => panic!(
            "Expected precise or global resolution, got {:?}",
            resolution
        ),
    }

    // 2. Find implementations of 'run'
    let query = SymbolQuery {
        language: naviscope_api::models::Language::JAVA,
        resolution: resolution.clone(),
    };
    let impls = handle.find_implementations(&query).await.unwrap();
    assert_eq!(impls.len(), 1);
    assert!(impls[0].path.to_string_lossy().contains("Impl.java"));

    // 3. Find incoming calls to 'Impl#run'
    // With semantic reference checks, searching for an implementation should find
    // calls to the base method as well.
    let calls = handle
        .find_incoming_calls("com.example.Impl#run")
        .await
        .unwrap();
    assert_eq!(
        calls.len(),
        1,
        "Lookup of Impl#run should find the call via Base type"
    );

    // 4. Find incoming calls to 'Base#run'
    let target_fqn = "com.example.Base#run";
    let incoming_base = handle.find_incoming_calls(target_fqn).await.unwrap();

    // Test find_references as a comparison
    let query_refs = ReferenceQuery {
        language: naviscope_api::models::Language::JAVA,
        resolution: resolution.clone(),
        include_declaration: false,
    };
    let refs = handle.find_references(&query_refs).await.unwrap();
    assert_eq!(
        refs.len(),
        1,
        "find_references should have found 1 reference in App.java"
    );

    assert_eq!(
        incoming_base.len(),
        1,
        "find_incoming_calls should have found 1 caller in App.java"
    );
    assert_eq!(incoming_base[0].from.id, "com.example.App#start");
}
