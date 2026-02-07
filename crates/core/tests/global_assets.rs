use naviscope_core::runtime::orchestrator::NaviscopeEngine;
use naviscope_java::JavaPlugin;
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test]
async fn test_global_asset_scan_produces_routes() {
    let dir = tempdir().unwrap();
    let java_plugin = Arc::new(JavaPlugin::new().expect("Failed to create JavaPlugin"));
    let engine = NaviscopeEngine::builder(dir.path().to_path_buf())
        .with_language(java_plugin)
        .build();

    let scan = engine
        .scan_global_assets()
        .await
        .expect("Expected asset service to be available");

    let routes = engine.global_asset_routes();

    assert!(
        scan.total_assets >= scan.indexed_assets + scan.skipped_assets + scan.failed_assets
    );

    if scan.total_assets > 0 {
        assert!(scan.total_prefixes > 0, "Expected some prefixes to be indexed");
        assert!(!routes.is_empty(), "Expected routes to be populated");
    }
}
