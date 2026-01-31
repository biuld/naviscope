use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::{CodeGraph, NaviscopeEngine as InternalEngine};
use crate::error::Result;
use crate::project::is_relevant_path;
use crate::project::watcher::Watcher;
use naviscope_api::NaviscopeEngine;

mod graph;
mod lifecycle;
mod navigation;
mod semantic;

/// Engine handle - unified interface for all clients
///
/// This provides both async and sync APIs:
/// - Async API: for LSP and MCP servers
/// - Sync API: for Shell REPL
#[derive(Clone)]
pub struct EngineHandle {
    pub(crate) engine: Arc<InternalEngine>,
}

impl EngineHandle {
    /// Create a new engine handle
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            engine: Arc::new(InternalEngine::new(project_root)),
        }
    }

    /// Create a handle from an existing engine (useful for testing)
    pub fn from_engine(engine: Arc<InternalEngine>) -> Self {
        Self { engine }
    }

    // ---- Async API (for LSP/MCP) ----

    /// Get a snapshot of the current graph (async)
    pub async fn graph(&self) -> CodeGraph {
        self.engine.snapshot().await
    }

    // ---- Language specific services (internal) ----

    pub fn get_lsp_parser(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn crate::parser::LspParser>> {
        self.engine.get_resolver().get_lsp_parser(language)
    }

    pub fn get_semantic_resolver(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn crate::resolver::SemanticResolver>> {
        self.engine.get_resolver().get_semantic_resolver(language)
    }

    pub fn get_feature_provider(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn naviscope_api::plugin::LanguageFeatureProvider>> {
        self.engine.get_resolver().get_feature_provider(language)
    }

    pub fn get_language_by_extension(&self, ext: &str) -> Option<crate::project::source::Language> {
        self.engine.get_resolver().get_language_by_extension(ext)
    }

    pub fn get_parser_and_lang_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<(
        Arc<dyn crate::parser::LspParser>,
        crate::project::source::Language,
    )> {
        let ext = path.extension()?.to_str()?;
        let lang = self.get_language_by_extension(ext)?;
        let parser = self.get_lsp_parser(lang.clone())?;
        Some((parser, lang))
    }

    // ---- File watching ----

    /// Watch for filesystem changes
    pub async fn watch(&self) -> Result<()> {
        let root = self.engine.root_path().to_path_buf();
        let engine = self.engine.clone();

        tokio::spawn(async move {
            let mut watcher = match Watcher::new(&root) {
                Ok(w) => w,
                Err(e) => {
                    tracing::error!("Failed to start watcher: {}", e);
                    return;
                }
            };

            tracing::info!("Started watching {}", root.display());

            let mut pending_events: Vec<notify::Event> = Vec::new();
            let debounce_interval = Duration::from_millis(500);

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
                        let mut paths = HashSet::new();
                        for event in &pending_events {
                            // Filter for modify/create/remove events to be safe?
                            // For now, accept all relevant file events.
                            for path in &event.paths {
                                // Basic relevance check (e.g. ignore .git, tmp)
                                // Assuming crate::project::is_relevant_path exists and is public
                                if is_relevant_path(path) {
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
}

impl NaviscopeEngine for EngineHandle {}

#[cfg(test)]
mod tests {
    use naviscope_api::GraphService;

    use super::*;

    #[tokio::test]
    async fn test_async_graph_access() {
        let engine = Arc::new(InternalEngine::new(PathBuf::from(".")));
        let handle = EngineHandle::from_engine(engine);

        let graph = handle.graph().await;
        assert_eq!(graph.node_count(), 0); // Empty initially
    }

    #[test]
    fn test_blocking_graph_access() {
        // Create runtime in a separate thread without any existing runtime context
        std::thread::spawn(|| {
            let engine = Arc::new(InternalEngine::new(PathBuf::from(".")));
            let handle = EngineHandle::from_engine(engine);

            // Test that blocking API works via async runtime
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let _graph = rt.block_on(handle.graph());
        })
        .join()
        .unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_queries() {
        use tokio::task::JoinSet;

        let engine = Arc::new(InternalEngine::new(PathBuf::from(".")));
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
        use naviscope_api::models::GraphQuery;

        let engine = Arc::new(InternalEngine::new(PathBuf::from(".")));
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
        use naviscope_api::GraphService;
        use naviscope_api::models::GraphQuery;

        std::thread::spawn(|| {
            let engine = Arc::new(InternalEngine::new(PathBuf::from(".")));
            let handle = EngineHandle::from_engine(engine);

            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let query = GraphQuery::Find {
                pattern: "test".to_string(),
                kind: vec![],
                limit: 10,
            };

            // Use trait method via async runtime
            let result = rt.block_on(handle.query(&query));
            assert!(result.is_ok(), "Blocking query should execute successfully");
        })
        .join()
        .unwrap();
    }
}
