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

    // ---- Async API (for LSP/MCP) ----

    /// Get a snapshot of the current graph (async)
    pub async fn graph(&self) -> CodeGraph {
        self.engine.snapshot().await
    }

    /// Execute a query (async)
    ///
    /// TODO: Update QueryEngine to work with engine::CodeGraph
    pub async fn query(&self, _query: &GraphQuery) -> Result<QueryResult> {
        // Temporarily disabled - needs QueryEngine refactor
        unimplemented!("Query functionality will be restored after QueryEngine refactor")

        // let graph = self.graph().await;
        // let query = query.clone();
        //
        // // Execute in blocking pool to avoid blocking async runtime
        // tokio::task::spawn_blocking(move || {
        //     let engine = QueryEngine::new(&graph);
        //     engine.execute(&query)
        // })
        // .await
        // .map_err(|e| crate::error::NaviscopeError::Internal(e.to_string()))?
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

    // ---- Sync API (for Shell) ----

    /// Get a snapshot of the current graph (sync)
    ///
    /// Note: This requires a tokio runtime to be available
    pub fn graph_blocking(&self) -> CodeGraph {
        tokio::runtime::Handle::current().block_on(self.engine.snapshot())
    }

    /// Execute a query (sync)
    ///
    /// TODO: Update QueryEngine to work with engine::CodeGraph
    pub fn query_blocking(&self, _query: &GraphQuery) -> Result<QueryResult> {
        // Temporarily disabled - needs QueryEngine refactor
        unimplemented!("Query functionality will be restored after QueryEngine refactor")

        // let graph = self.graph_blocking();
        // let engine = QueryEngine::new(&graph);
        // engine.execute(query)
    }

    /// Rebuild the index (sync)
    pub fn rebuild_blocking(&self) -> Result<()> {
        tokio::runtime::Handle::current().block_on(self.engine.rebuild())
    }

    // ---- File watching ----

    /// Start watching for file changes (async)
    ///
    /// This spawns a background task that monitors file system changes
    /// and automatically updates the index.
    pub async fn watch(&self) -> Result<()> {
        // TODO: Implement file watching
        // For now, just return Ok
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_async_graph_access() {
        let handle = EngineHandle::new(PathBuf::from("."));
        let graph = handle.graph().await;
        assert_eq!(graph.node_count(), 0);
    }

    #[tokio::test]
    async fn test_blocking_graph_access() {
        let handle = EngineHandle::new(PathBuf::from("."));

        // Spawn a blocking task
        tokio::task::spawn_blocking(move || {
            let graph = handle.graph_blocking();
            assert_eq!(graph.node_count(), 0);
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_queries() {
        use tokio::task::JoinSet;

        let handle = EngineHandle::new(PathBuf::from("."));

        let mut set = JoinSet::new();

        for _ in 0..10 {
            let h = handle.clone();
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
