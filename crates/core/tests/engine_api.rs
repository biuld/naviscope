use naviscope_core::engine::{EngineHandle, NaviscopeEngine};
use std::sync::Arc;

#[tokio::test]
async fn test_engine_lifecycle() {
    let temp_dir = std::env::temp_dir().join("naviscope_test_engine");
    if temp_dir.exists() {
        let _ = std::fs::remove_dir_all(&temp_dir);
    }
    std::fs::create_dir_all(&temp_dir).unwrap();

    // Use EngineHandle::new which takes project_root
    let handle = EngineHandle::new(temp_dir.clone());

    // Get a snapshot using handle.graph()
    let graph = handle.graph().await;
    assert_eq!(graph.node_count(), 0);

    // Verify handle can be cloned easily
    let handle2 = handle.clone();
    let graph2 = handle2.graph().await;
    assert_eq!(graph2.node_count(), 0);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_engine_handle_query() {
    use naviscope_core::query::GraphQuery;

    let temp_dir = std::env::temp_dir().join("naviscope_test_query");
    std::fs::create_dir_all(&temp_dir).ok();

    let engine = Arc::new(NaviscopeEngine::new(temp_dir.clone()));
    let handle = EngineHandle::from_engine(engine);

    // Test query execution via handle
    let query = GraphQuery::Find {
        pattern: "test".to_string(),
        kind: vec![],
        limit: 5,
    };

    let result = handle.query(&query).await;
    assert!(result.is_ok());

    let _ = std::fs::remove_dir_all(&temp_dir);
}
