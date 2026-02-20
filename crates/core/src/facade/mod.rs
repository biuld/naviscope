use std::path::PathBuf;
use std::sync::Arc;

use crate::error::Result;
use crate::model::CodeGraph;
use crate::runtime::NaviscopeEngine as InternalEngine;
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
            engine: Arc::new(InternalEngine::builder(project_root).build()),
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

    pub fn get_semantic_resolver(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::SemanticCap>> {
        self.engine.semantic_cap(language)
    }

    pub fn get_node_presenter(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::NodePresenter>> {
        self.engine.node_presenter(language)
    }

    pub fn get_metadata_codec(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::NodeMetadataCodec>> {
        self.engine.metadata_codec(language)
    }

    pub fn get_language_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<crate::model::source::Language> {
        self.engine.language_for_path(path)
    }

    pub fn get_services_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<(
        Arc<dyn naviscope_plugin::SemanticCap>,
        crate::model::source::Language,
    )> {
        let lang = self.get_language_for_path(path)?;
        let semantic_cap = self.get_semantic_resolver(lang.clone())?;
        Some((semantic_cap, lang))
    }

    /// Get naming convention for a specific language
    pub fn get_naming_convention(
        &self,
        language: &str,
    ) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        self.engine.naming_conventions().get(language).cloned()
    }

    /// Get all naming conventions (cheap Arc clone)
    pub(crate) fn naming_conventions(
        &self,
    ) -> Arc<std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>> {
        self.engine.naming_conventions()
    }

    // ---- File watching ----

    /// Watch for filesystem changes
    pub async fn watch(&self) -> Result<()> {
        self.engine.clone().watch().await
    }
}

impl NaviscopeEngine for EngineHandle {
    fn get_stub_cache_manager(&self) -> Arc<dyn naviscope_api::StubCacheManager> {
        self.engine.get_stub_cache()
    }
}

#[cfg(test)]
mod tests {
    use naviscope_api::GraphService;

    use super::*;

    #[tokio::test]
    async fn test_async_graph_access() {
        let engine = Arc::new(InternalEngine::builder(PathBuf::from(".")).build());
        let handle = EngineHandle::from_engine(engine);

        let graph = handle.graph().await;
        assert_eq!(graph.node_count(), 0); // Empty initially
    }

    #[test]
    fn test_blocking_graph_access() {
        // Create runtime in a separate thread without any existing runtime context
        std::thread::spawn(|| {
            // Test that blocking API works via async runtime
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let engine = Arc::new(InternalEngine::builder(PathBuf::from(".")).build());
            let handle = EngineHandle::from_engine(engine);

            let _graph = rt.block_on(handle.graph());
        })
        .join()
        .unwrap();
    }

    #[tokio::test]
    async fn test_concurrent_queries() {
        use tokio::task::JoinSet;

        let engine = Arc::new(InternalEngine::builder(PathBuf::from(".")).build());
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

        let engine = Arc::new(InternalEngine::builder(PathBuf::from(".")).build());
        let handle = EngineHandle::from_engine(engine);

        // Test async query
        let query = GraphQuery::Find {
            pattern: "test".to_string(),
            kind: vec![],
            sources: vec![],
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
            let rt = tokio::runtime::Runtime::new().unwrap();
            let _guard = rt.enter();

            let engine = Arc::new(InternalEngine::builder(PathBuf::from(".")).build());
            let handle = EngineHandle::from_engine(engine);

            let query = GraphQuery::Find {
                pattern: "test".to_string(),
                kind: vec![],
                sources: vec![],
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
