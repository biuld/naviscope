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
    pub async fn query(&self, query: &GraphQuery) -> Result<QueryResult> {
        let graph = self.graph().await; // Arc clone - cheap
        let query_owned = query.clone();

        // Execute in blocking pool to avoid blocking async runtime
        // Since graph is owned (Arc), we can safely move it into the closure
        let result = tokio::task::spawn_blocking(move || -> Result<QueryResult> {
            // Create QueryEngine with owned graph - no lifetime issues!
            let engine = crate::query::QueryEngine::new(graph);
            engine.execute(&query_owned)
        })
        .await
        .map_err(|e| crate::error::NaviscopeError::Internal(e.to_string()))??;

        Ok(result)
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
        let root = self.engine.root_path().to_path_buf();
        let engine = self.engine.clone();

        tokio::spawn(async move {
            let mut watcher = match crate::project::watcher::Watcher::new(&root) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("Failed to start watcher: {}", e);
                    return;
                }
            };

            tracing::info!("Started watching {}", root.display());

            let mut pending_events: Vec<notify::Event> = Vec::new();
            let debounce_interval = std::time::Duration::from_millis(500);

            loop {
                tokio::select! {
                    event = watcher.next_event_async() => {
                        match event {
                            Some(e) => pending_events.push(e),
                            None => break, // Channel closed
                        }
                    }
                    _ = tokio::time::sleep(debounce_interval), if !pending_events.is_empty() => {
                        // Extract unique paths
                        let mut paths = std::collections::HashSet::new();
                        for event in &pending_events {
                            // Filter for modify/create/remove events to be safe?
                            // For now, accept all relevant file events.
                            for path in &event.paths {
                                // Basic relevance check (e.g. ignore .git, tmp)
                                // Assuming crate::project::is_relevant_path exists and is public
                                if crate::project::is_relevant_path(path) {
                                     paths.insert(path.clone());
                                }
                            }
                        }

                        pending_events.clear();

                        if !paths.is_empty() {
                            let path_vec: Vec<_> = paths.into_iter().collect();
                            tracing::info!("Detected changes in {} files. Updating...", path_vec.len());
                            if let Err(e) = engine.update_files(path_vec).await {
                                tracing::error!("Failed to update index: {}", e);
                            } else {
                                tracing::info!("Index updated successfully.");
                            }
                        }
                    }
                }
            }
        });

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
    pub fn query_blocking(&self, query: &GraphQuery) -> Result<QueryResult> {
        let graph = self.graph_blocking();
        // Use the generic QueryEngine - it owns the graph
        let engine = crate::query::QueryEngine::new(graph);
        engine.execute(query)
    }

    /// Rebuild the index (sync)
    pub fn rebuild_blocking(&self) -> Result<()> {
        tokio::runtime::Handle::current().block_on(self.rebuild())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[tokio::test]
    async fn test_query_functionality() {
        use crate::query::GraphQuery;

        let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));
        let handle = EngineHandle::from_engine(engine);

        // Test async query
        let query = GraphQuery::Find {
            pattern: "test".to_string(),
            kind: vec![],
            limit: 10,
        };

        let result = handle.query(&query).await;
        assert!(result.is_ok(), "Query should execute successfully");
    }

    #[test]
    fn test_query_blocking() {
        use crate::query::GraphQuery;

        std::thread::spawn(|| {
            let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));
            let handle = EngineHandle::from_engine(engine);

            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let query = GraphQuery::Find {
                pattern: "test".to_string(),
                kind: vec![],
                limit: 10,
            };

            let result = handle.query_blocking(&query);
            assert!(result.is_ok(), "Blocking query should execute successfully");
        })
        .join()
        .unwrap();
    }
}
