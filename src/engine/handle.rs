//! Unified engine handle for all clients

use super::{CodeGraph, NaviscopeEngine};
use crate::error::Result;
use crate::query::{GraphQuery, QueryResult};
use std::path::PathBuf;
use std::sync::Arc;

/// Engine handle - unified interface for all clients
///
/// This provides both async and sync APIs:
/// - Async API: for LSP and MCP servers
/// - Sync API: for Shell REPL
#[derive(Clone)]
pub struct EngineHandle {
    engine: Arc<NaviscopeEngine>,
}

impl EngineHandle {
    /// Create a new engine handle
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            engine: Arc::new(NaviscopeEngine::new(project_root)),
        }
    }

    /// Create a handle from an existing engine (useful for testing)
    pub fn from_engine(engine: Arc<NaviscopeEngine>) -> Self {
        Self { engine }
    }

    // ---- Async API (for LSP/MCP) ----

    /// Get a snapshot of the current graph (async)
    pub async fn graph(&self) -> CodeGraph {
        self.engine.snapshot().await
    }

    /// Execute a query (async)
    ///
    /// Note: Query functionality temporarily disabled pending QueryEngine refactor
    /// The trait object lifetime issue needs to be resolved in Phase 2
    pub async fn query(&self, _query: &GraphQuery) -> Result<QueryResult> {
        // TODO Phase 2: Implement query with proper trait object handling
        // See: https://github.com/rust-lang/rust/issues/96097
        unimplemented!("Query functionality will be restored in Phase 2")
    }

    /// Rebuild the index (async)
    pub async fn rebuild(&self) -> Result<()> {
        self.engine.rebuild().await
    }

    /// Load index from disk (async)
    pub async fn load(&self) -> Result<bool> {
        self.engine.load().await
    }

    /// Save index to disk (async)
    pub async fn save(&self) -> Result<()> {
        self.engine.save().await
    }

    /// Refresh the index (async)
    pub async fn refresh(&self) -> Result<()> {
        self.engine.refresh().await
    }

    // ---- File watching ----

    /// Watch for filesystem changes
    pub async fn watch(&self) -> Result<()> {
        // TODO: Implement file watching
        Ok(())
    }

    // ---- Sync API (for Shell) ----

    /// Get a snapshot of the current graph (sync)
    ///
    /// Note: This requires a tokio runtime to be available
    pub fn graph_blocking(&self) -> CodeGraph {
        tokio::runtime::Handle::current().block_on(self.graph())
    }

    /// Execute a query (sync)
    ///
    /// Note: Query functionality temporarily disabled pending QueryEngine refactor  
    pub fn query_blocking(&self, _query: &GraphQuery) -> Result<QueryResult> {
        unimplemented!("Query functionality will be restored in Phase 2")
    }

    /// Rebuild the index (sync)
    pub fn rebuild_blocking(&self) -> Result<()> {
        tokio::runtime::Handle::current().block_on(self.rebuild())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[tokio::test]
    async fn test_async_graph_access() {
        let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));
        let handle = EngineHandle::from_engine(engine);

        let graph = handle.graph().await;
        assert_eq!(graph.node_count(), 0); // Empty initially
    }

    #[test]
    fn test_blocking_graph_access() {
        // Create runtime in a separate thread without any existing runtime context
        std::thread::spawn(|| {
            let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));
            let handle = EngineHandle::from_engine(engine);

            // Test that blocking API works
            // Note: graph_blocking requires a tokio runtime, so we need to set one up
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let _graph = handle.graph_blocking();
        })
        .join()
        .unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_queries() {
        use tokio::task::JoinSet;

        let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));
        let handle = Arc::new(EngineHandle::from_engine(engine));

        let mut set = JoinSet::new();

        for _ in 0..10 {
            let h = Arc::clone(&handle);
            set.spawn(async move {
                for _ in 0..5 {
                    let graph = h.graph().await;
                    let _ = graph.node_count();
                }
            });
        }

        while let Some(result) = set.join_next().await {
            result.unwrap();
        }
    }
}
