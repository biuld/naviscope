// ... existing code ...

#[async_trait]
impl GraphService for EngineHandle {
    async fn query(
        &self,
        query: &naviscope_api::models::GraphQuery,
    ) -> GraphResult<naviscope_api::models::QueryResult> {
        let graph = self.graph().await;
        // Need to convert API GraphQuery to Internal GraphQuery?
        // Or make core::query use API GraphQuery?
        // Let's serialize/deserialize as a quick hack or implement From/Into.
        // But core shouldn't depend on API if API depends on Core? API doesn't depend on core. Core depends on API.
        // So Core has access to API types.
        // We can map API GraphQuery to Core GraphQuery.

        let core_query = match query {
            naviscope_api::models::GraphQuery::Ls {
                fqn,
                kind,
                modifiers,
            } => crate::query::GraphQuery::Ls {
                fqn: fqn.clone(),
                kind: kind.clone(),
                modifiers: modifiers.clone(),
            },
            naviscope_api::models::GraphQuery::Find {
                pattern,
                kind,
                limit,
            } => crate::query::GraphQuery::Find {
                pattern: pattern.clone(),
                kind: kind.clone(),
                limit: *limit,
            },
            naviscope_api::models::GraphQuery::Cat { fqn } => {
                crate::query::GraphQuery::Cat { fqn: fqn.clone() }
            }
            naviscope_api::models::GraphQuery::Deps {
                fqn,
                rev,
                edge_types,
            } => crate::query::GraphQuery::Deps {
                fqn: fqn.clone(),
                rev: *rev,
                edge_types: edge_types.clone(),
            },
        };

        let result = tokio::task::spawn_blocking(move || -> Result<crate::query::QueryResult> {
            let engine = crate::query::QueryEngine::new(graph);
            engine.execute(&core_query)
        })
        .await
        .map_err(|e| naviscope_api::graph::GraphError::Internal(e.to_string()))?
        .map_err(|e| naviscope_api::graph::GraphError::Internal(e.to_string()))?;

        // Convert Core QueryResult to API QueryResult
        let api_nodes = result.nodes.iter().map(|n| n.to_api()).collect();
        let api_edges = result
            .edges
            .iter()
            .map(|e| naviscope_api::models::QueryResultEdge {
                from: e.from.to_string(),
                to: e.to.to_string(),
                data: e.data.clone(),
            })
            .collect();

        Ok(naviscope_api::models::QueryResult::new(
            api_nodes, api_edges,
        ))
    }

    async fn get_stats(&self) -> GraphResult<naviscope_api::graph::GraphStats> {
        let graph = self.graph().await;
        Ok(naviscope_api::graph::GraphStats {
            node_count: graph.topology().node_count(),
            edge_count: graph.topology().edge_count(),
        })
    }
}

use super::{CodeGraph, LanguageService, NaviscopeEngine as InternalEngine};
use crate::error::Result;
use crate::query::{GraphQuery, QueryResult};
use async_trait::async_trait;
use naviscope_api::NaviscopeEngine;
use naviscope_api::graph::{GraphService, Result as GraphResult};
use naviscope_api::lifecycle::{EngineError, EngineLifecycle, Result as LifecycleResult};
use naviscope_api::models::Language;
use naviscope_api::models::{
    PositionContext, ReferenceQuery, SymbolInfo, SymbolLocation, SymbolQuery, SymbolResolution,
};
use naviscope_api::plugin::LanguageFeatureProvider;
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, Result as SemanticResult, SemanticError,
    SymbolInfoProvider, SymbolNavigator,
};

use std::path::PathBuf;
use std::sync::Arc;

/// Engine handle - unified interface for all clients
///
/// This provides both async and sync APIs:
/// - Async API: for LSP and MCP servers
/// - Sync API: for Shell REPL
#[derive(Clone)]
pub struct EngineHandle {
    engine: Arc<InternalEngine>,
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
    // ---- Language Services (Sync/Inherent) ----

    pub fn get_lsp_parser(&self, language: Language) -> Option<Arc<dyn crate::parser::LspParser>> {
        self.engine.get_resolver().get_lsp_parser(language)
    }

    pub fn get_semantic_resolver(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::resolver::SemanticResolver>> {
        self.engine.get_resolver().get_semantic_resolver(language)
    }

    pub fn get_feature_provider(
        &self,
        language: Language,
    ) -> Option<Arc<dyn LanguageFeatureProvider>> {
        self.engine.get_resolver().get_feature_provider(language)
    }

    pub fn get_language_by_extension(&self, ext: &str) -> Option<Language> {
        self.engine.get_resolver().get_language_by_extension(ext)
    }

    /// Get parser and language for a file path (convenience method)
    pub fn get_parser_and_lang_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<(Arc<dyn crate::parser::LspParser>, Language)> {
        let ext = path.extension()?.to_str()?;
        let lang = self.get_language_by_extension(ext)?;
        let parser = self.get_lsp_parser(lang.clone())?;
        Some((parser, lang))
    }
}

// Implement LanguageService trait for EngineHandle (using Core types)
impl LanguageService for EngineHandle {
    fn get_lsp_parser(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn crate::parser::LspParser>> {
        self.engine.get_resolver().get_lsp_parser(language)
    }

    fn get_semantic_resolver(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn crate::resolver::SemanticResolver>> {
        self.engine.get_resolver().get_semantic_resolver(language)
    }

    fn get_feature_provider(
        &self,
        language: crate::project::source::Language,
    ) -> Option<Arc<dyn crate::plugin::LanguageFeatureProvider>> {
        self.engine.get_resolver().get_feature_provider(language)
    }

    fn get_language_by_extension(&self, ext: &str) -> Option<crate::project::source::Language> {
        self.engine.get_resolver().get_language_by_extension(ext)
    }
}

#[cfg(test)]
mod tests {
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
        use crate::query::GraphQuery;

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
        use crate::query::GraphQuery;

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

            let result = handle.query_blocking(&query);
            assert!(result.is_ok(), "Blocking query should execute successfully");
        })
        .join()
        .unwrap();
    }
}

// ============================================================================
// Semantic Service Implementations - Split into Focused Traits
// ============================================================================

#[async_trait]
impl SymbolNavigator for EngineHandle {
    async fn resolve_symbol_at(
        &self,
        ctx: &PositionContext,
    ) -> SemanticResult<Option<SymbolResolution>> {
        let uri_str = &ctx.uri;
        let path = if uri_str.starts_with("file://") {
            PathBuf::from(uri_str.strip_prefix("file://").unwrap())
        } else {
            PathBuf::from(uri_str)
        };

        let (parser, lang) = match self.get_parser_and_lang_for_path(&path) {
            Some(x) => x,
            None => return Ok(None),
        };

        let resolver = match self.get_semantic_resolver(lang.clone()) {
            Some(r) => r,
            None => return Ok(None),
        };

        let content = if let Some(c) = &ctx.content {
            c.clone()
        } else {
            std::fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?
        };

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

        let byte_col =
            crate::util::utf16_col_to_byte_col(&content, ctx.line as usize, ctx.char as usize);

        let graph = self.graph().await;

        Ok(resolver.resolve_at(&tree, &content, ctx.line as usize, byte_col, &graph))
    }

    async fn find_highlights(
        &self,
        ctx: &PositionContext,
    ) -> SemanticResult<Vec<naviscope_api::models::Range>> {
        let uri_str = &ctx.uri;
        let path = if uri_str.starts_with("file://") {
            PathBuf::from(uri_str.strip_prefix("file://").unwrap())
        } else {
            PathBuf::from(uri_str)
        };

        let (parser, _) = match self.get_parser_and_lang_for_path(&path) {
            Some(x) => x,
            None => return Ok(vec![]),
        };

        let content = if let Some(c) = &ctx.content {
            c.clone()
        } else {
            std::fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?
        };

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

        let res = match self.resolve_symbol_at(ctx).await? {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        Ok(parser.find_occurrences(&content, &tree, &res))
    }

    async fn find_definitions(&self, query: &SymbolQuery) -> SemanticResult<Vec<SymbolLocation>> {
        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let graph = self.graph().await;
        let matches = resolver.find_matches(&graph, &query.resolution);

        let topology = graph.topology();
        let mut locations = Vec::new();

        for idx in matches {
            let node = &topology[idx];
            if let (Some(path), Some(range)) = (node.file_path(), node.range()) {
                locations.push(SymbolLocation {
                    path: path.to_path_buf(),
                    range: range.clone(),
                    fqn: node.fqn().to_string(),
                });
            }
        }
        Ok(locations)
    }

    async fn find_type_definitions(
        &self,
        query: &SymbolQuery,
    ) -> SemanticResult<Vec<SymbolLocation>> {
        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => return Ok(vec![]),
        };
        let graph = self.graph().await;

        let type_resolutions = resolver.resolve_type_of(&graph, &query.resolution);
        let topology = graph.topology();
        let mut locations = Vec::new();

        for res in type_resolutions {
            let matches = resolver.find_matches(&graph, &res);
            for idx in matches {
                let node = &topology[idx];
                if let (Some(path), Some(range)) = (node.file_path(), node.range()) {
                    locations.push(SymbolLocation {
                        path: path.to_path_buf(),
                        range: range.clone(),
                        fqn: node.fqn().to_string(),
                    });
                }
            }
        }
        Ok(locations)
    }

    async fn find_implementations(
        &self,
        query: &SymbolQuery,
    ) -> SemanticResult<Vec<SymbolLocation>> {
        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => return Ok(vec![]),
        };
        let graph = self.graph().await;
        let matches = resolver.find_implementations(&graph, &query.resolution);

        let topology = graph.topology();
        let mut locations = Vec::new();

        for idx in matches {
            let node = &topology[idx];
            if let (Some(path), Some(range)) = (node.file_path(), node.range()) {
                locations.push(SymbolLocation {
                    path: path.to_path_buf(),
                    range: range.clone(),
                    fqn: node.fqn().to_string(),
                });
            }
        }
        Ok(locations)
    }
}

#[async_trait]
impl ReferenceAnalyzer for EngineHandle {
    async fn find_references(&self, query: &ReferenceQuery) -> SemanticResult<Vec<SymbolLocation>> {
        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => return Ok(vec![]),
        };
        let graph = self.graph().await;

        let matches = resolver.find_matches(&graph, &query.resolution);
        let discovery = crate::analysis::discovery::DiscoveryEngine::new(&graph);
        let candidate_paths = discovery.scout_references(&matches);

        let mut tasks = tokio::task::JoinSet::new();

        for path in candidate_paths {
            let handle = self.clone();
            let resolution = query.resolution.clone();
            let _lang = query.language.clone();

            tasks.spawn(async move {
                let (parser, file_lang) = match handle.get_parser_and_lang_for_path(&path) {
                    Some(x) => x,
                    None => return Vec::new(),
                };

                let file_resolver = match handle.get_semantic_resolver(file_lang) {
                    Some(r) => r,
                    None => return Vec::new(),
                };

                let content = match std::fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => return Vec::new(),
                };

                let graph = handle.graph().await;
                let discovery = crate::analysis::discovery::DiscoveryEngine::new(&graph);

                let uri_str = format!("file://{}", path.display());
                let uri = match url::Url::parse(&uri_str) {
                    Ok(u) => u,
                    Err(_) => return Vec::new(),
                };

                let locations = discovery.scan_file(
                    parser.as_ref(),
                    file_resolver.as_ref(),
                    &content,
                    &resolution,
                    &uri,
                );

                locations
                    .into_iter()
                    .map(|loc| {
                        let path_buf = loc.uri.to_file_path().unwrap();
                        SymbolLocation {
                            path: path_buf,
                            range: naviscope_api::models::Range {
                                start_line: loc.range.start.line as usize,
                                start_col: loc.range.start.character as usize,
                                end_line: loc.range.end.line as usize,
                                end_col: loc.range.end.character as usize,
                            },
                            fqn: "".to_string(),
                        }
                    })
                    .collect::<Vec<_>>()
            });
        }

        let mut all_locations = Vec::new();
        while let Some(res) = tasks.join_next().await {
            if let Ok(locs) = res {
                all_locations.extend(locs);
            }
        }

        all_locations.sort_by(|a, b| {
            a.path
                .cmp(&b.path)
                .then(a.range.start_line.cmp(&b.range.start_line))
                .then(a.range.start_col.cmp(&b.range.start_col))
        });
        all_locations.dedup_by(|a, b| {
            a.path == b.path
                && a.range.start_line == b.range.start_line
                && a.range.start_col == b.range.start_col
        });

        Ok(all_locations)
    }
}

#[async_trait]
impl CallHierarchyAnalyzer for EngineHandle {
    async fn find_incoming_calls(
        &self,
        _fqn: &str,
    ) -> SemanticResult<Vec<naviscope_api::models::CallHierarchyIncomingCall>> {
        // Placeholder
        Ok(vec![])
    }

    async fn find_outgoing_calls(
        &self,
        _fqn: &str,
    ) -> SemanticResult<Vec<naviscope_api::models::CallHierarchyOutgoingCall>> {
        // Placeholder
        Ok(vec![])
    }
}

#[async_trait]
impl SymbolInfoProvider for EngineHandle {
    async fn get_symbol_info(&self, _fqn: &str) -> SemanticResult<Option<SymbolInfo>> {
        // Placeholder
        Ok(None)
    }

    async fn get_document_symbols(
        &self,
        uri: &str,
    ) -> SemanticResult<Vec<naviscope_api::models::DocumentSymbol>> {
        let path = if uri.starts_with("file://") {
            PathBuf::from(uri.strip_prefix("file://").unwrap())
        } else {
            PathBuf::from(uri)
        };

        let (parser, _) = match self.get_parser_and_lang_for_path(&path) {
            Some(x) => x,
            None => return Ok(vec![]),
        };

        let content =
            std::fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

        Ok(parser.extract_symbols(&tree, &content))
    }

    async fn get_language_for_document(&self, uri: &str) -> SemanticResult<Option<Language>> {
        let path = if uri.starts_with("file://") {
            PathBuf::from(uri.strip_prefix("file://").unwrap())
        } else {
            PathBuf::from(uri)
        };

        let ext = match path.extension().and_then(|e| e.to_str()) {
            Some(e) => e,
            None => return Ok(None),
        };

        Ok(self.get_language_by_extension(ext))
    }
}

// ============================================================================
// Navigation Service Implementation - CLI-Style Path Resolution
// ============================================================================

#[async_trait]
impl naviscope_api::navigation::NavigationService for EngineHandle {
    async fn resolve_path(
        &self,
        target: &str,
        current_context: Option<&str>,
    ) -> naviscope_api::navigation::ResolveResult {
        use naviscope_api::navigation::ResolveResult;

        // 1. Handle special paths ("/" or "root")
        if target == "/" || target == "root" {
            let graph = self.graph().await;
            use crate::model::graph::NodeKind;

            let project_nodes: Vec<_> = graph
                .topology()
                .node_indices()
                .filter_map(|idx| {
                    let node = &graph.topology()[idx];
                    if matches!(node.kind(), NodeKind::Project) {
                        Some(node.fqn().to_string())
                    } else {
                        None
                    }
                })
                .collect();

            return match project_nodes.len() {
                1 => ResolveResult::Found(project_nodes[0].clone()),
                0 => ResolveResult::Found("".to_string()),
                _ => ResolveResult::Ambiguous(project_nodes),
            };
        }

        let graph = self.graph().await;

        // 2. Handle parent navigation ("..")
        if target == ".." {
            if let Some(current_fqn) = current_context {
                if let Some(&idx) = graph.fqn_map().get(current_fqn) {
                    use crate::model::graph::EdgeType;
                    let mut incoming = graph
                        .topology()
                        .neighbors_directed(idx, petgraph::Direction::Incoming)
                        .detach();

                    while let Some((edge_idx, neighbor_idx)) = incoming.next(graph.topology()) {
                        let edge = &graph.topology()[edge_idx];
                        if edge.edge_type == EdgeType::Contains {
                            if let Some(parent_node) = graph.topology().node_weight(neighbor_idx) {
                                return ResolveResult::Found(parent_node.fqn().to_string());
                            }
                        }
                    }
                }
            }
            return ResolveResult::NotFound;
        }

        // 3. Try exact match (absolute FQN)
        if graph.fqn_map().contains_key(target) {
            return ResolveResult::Found(target.to_string());
        }

        // 4. Try relative path from current context
        if let Some(current_fqn) = current_context {
            let separator = if current_fqn.contains("::") {
                "::"
            } else {
                "."
            };
            let joined = format!("{}{}{}", current_fqn, separator, target);
            if graph.fqn_map().contains_key(joined.as_str()) {
                return ResolveResult::Found(joined);
            }
        }

        // 5. Try fuzzy matching (child lookup)
        let current_idx = current_context
            .and_then(|fqn| graph.fqn_map().get(fqn))
            .copied();

        let candidates: Vec<String> = if let Some(parent_idx) = current_idx {
            // Search in children of current node
            use crate::model::graph::EdgeType;
            graph
                .topology()
                .neighbors_directed(parent_idx, petgraph::Direction::Outgoing)
                .filter_map(|child_idx| {
                    // Check if edge is "Contains"
                    let edge_idx = graph.topology().find_edge(parent_idx, child_idx).unwrap();
                    let edge = &graph.topology()[edge_idx];

                    if edge.edge_type == EdgeType::Contains {
                        let node = &graph.topology()[child_idx];
                        let fqn = node.fqn();

                        // Match by simple name (last component)
                        let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                        if simple_name == target {
                            Some(fqn.to_string())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect()
        } else {
            // Global fuzzy search
            graph
                .fqn_map()
                .keys()
                .filter(|fqn| {
                    let simple_name = fqn.split(&['.', ':']).last().unwrap_or(fqn);
                    simple_name == target
                })
                .map(|s| s.to_string())
                .collect()
        };

        match candidates.len() {
            0 => ResolveResult::NotFound,
            1 => ResolveResult::Found(candidates[0].clone()),
            _ => ResolveResult::Ambiguous(candidates),
        }
    }

    async fn get_completion_candidates(&self, prefix: &str) -> Vec<String> {
        let graph = self.graph().await;
        graph
            .fqn_map()
            .keys()
            .filter(|fqn| fqn.starts_with(prefix))
            .take(50) // Reasonable limit for candidates
            .map(|s| s.to_string())
            .collect()
    }
}

#[async_trait]
impl EngineLifecycle for EngineHandle {
    async fn rebuild(&self) -> LifecycleResult<()> {
        self.engine
            .rebuild()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn load(&self) -> LifecycleResult<bool> {
        self.engine
            .load()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn save(&self) -> LifecycleResult<()> {
        self.engine
            .save()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn refresh(&self) -> LifecycleResult<()> {
        self.engine
            .refresh()
            .await
            .map_err(|e| EngineError::Internal(e.to_string()))
    }

    async fn watch(&self) -> LifecycleResult<()> {
        self.engine
            .watch()
            .await
            .map_err(|e: crate::error::NaviscopeError| EngineError::Internal(e.to_string()))
    }

    async fn clear_index(&self) -> LifecycleResult<()> {
        self.engine
            .clear_project_index()
            .await
            .map_err(|e: crate::error::NaviscopeError| EngineError::Internal(e.to_string()))
    }

    fn get_feature_provider(&self, language: Language) -> Option<Arc<dyn LanguageFeatureProvider>> {
        self.engine.get_resolver().get_feature_provider(language)
    }
}

impl NaviscopeEngine for EngineHandle {}
