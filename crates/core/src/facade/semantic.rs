use crate::facade::EngineHandle;
use crate::features::CodeGraphLike;
use crate::features::discovery::DiscoveryEngine;
use crate::util::utf16_col_to_byte_col;
use async_trait::async_trait;
use naviscope_api::graph::GraphService;
use naviscope_api::{ApiError, ApiResult};

use naviscope_api::models::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, DisplayGraphNode, Language, NodeKind,
    PositionContext, Range, ReferenceQuery, SymbolLocation, SymbolQuery, SymbolResolution,
};
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, SymbolInfoProvider, SymbolNavigator,
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::time::{Duration, sleep};

fn path_from_uri_like(uri: &str) -> PathBuf {
    if uri.starts_with("file://") {
        PathBuf::from(uri.strip_prefix("file://").unwrap_or(uri))
    } else {
        PathBuf::from(uri)
    }
}

impl EngineHandle {
    async fn hydrate_symbol_if_missing(&self, fqn: &str) -> ApiResult<()> {
        if self
            .get_node_display(fqn)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))?
            .is_some()
        {
            return Ok(());
        }

        if !self.engine.request_stub_for_fqn(fqn) {
            let _ = self.engine.scan_global_assets().await;
            let _ = self.engine.request_stub_for_fqn(fqn);
        }

        for _ in 0..3 {
            sleep(Duration::from_millis(25)).await;
            if self
                .get_node_display(fqn)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
                .is_some()
            {
                break;
            }
        }

        Ok(())
    }

    async fn hydrate_resolution_if_needed(&self, resolution: &SymbolResolution) -> ApiResult<()> {
        if let Some(fqn) = resolution.fqn() {
            self.hydrate_symbol_if_missing(fqn).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl SymbolNavigator for EngineHandle {
    async fn resolve_symbol_at(
        &self,
        ctx: &PositionContext,
    ) -> ApiResult<Option<SymbolResolution>> {
        let path = path_from_uri_like(&ctx.uri);

        let (semantic, _lang) = match self.get_services_for_path(&path) {
            Some(x) => x,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.resolve_symbol_at",
                    language: "unknown".to_string(),
                });
            }
        };

        let content = if let Some(c) = &ctx.content {
            c.clone()
        } else {
            fs::read_to_string(&path).map_err(|e| ApiError::Internal(e.to_string()))?
        };

        let tree = semantic
            .parse(&content, None)
            .ok_or_else(|| ApiError::Internal("Failed to parse".into()))?;

        let byte_col = utf16_col_to_byte_col(&content, ctx.line as usize, ctx.char as usize);

        let graph = self.graph().await;

        Ok(semantic.resolve_at(&tree, &content, ctx.line as usize, byte_col, &graph))
    }

    async fn find_highlights(&self, ctx: &PositionContext) -> ApiResult<Vec<Range>> {
        let path = path_from_uri_like(&ctx.uri);

        let (semantic, _) = match self.get_services_for_path(&path) {
            Some(x) => x,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.find_highlights",
                    language: "unknown".to_string(),
                });
            }
        };

        let content = if let Some(c) = &ctx.content {
            c.clone()
        } else {
            fs::read_to_string(&path).map_err(|e| ApiError::Internal(e.to_string()))?
        };

        let tree = semantic
            .parse(&content, None)
            .ok_or_else(|| ApiError::Internal("Failed to parse".into()))?;

        let res = match self.resolve_symbol_at(ctx).await? {
            Some(r) => r,
            None => return Ok(vec![]),
        };

        let graph = self.graph().await;
        Ok(semantic.find_occurrences(&content, &tree, &res, Some(&graph)))
    }

    async fn find_definitions(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>> {
        self.hydrate_resolution_if_needed(&query.resolution).await?;

        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.find_definitions",
                    language: query.language.to_string(),
                });
            }
        };

        let graph = self.graph().await;
        let matches = resolver.find_matches(&graph, &query.resolution);

        let topology = graph.topology();
        let mut locations = Vec::new();

        for fqn_id in matches {
            if let Some(&idx) = graph.fqn_map().get(&fqn_id) {
                let node = &topology[idx];
                if let Some(loc) = &node.location {
                    let path_str = graph.symbols().resolve(&loc.path.0);
                    locations.push(SymbolLocation {
                        path: Arc::from(PathBuf::from(path_str)),
                        range: loc.range,
                        selection_range: loc.selection_range,
                    });
                }
            }
        }
        Ok(locations)
    }

    async fn find_type_definitions(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>> {
        self.hydrate_resolution_if_needed(&query.resolution).await?;

        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.find_type_definitions",
                    language: query.language.to_string(),
                });
            }
        };
        let graph = self.graph().await;

        let type_resolutions = resolver.resolve_type_of(&graph, &query.resolution);
        let topology = graph.topology();
        let mut locations = Vec::new();

        for res in type_resolutions {
            let matches = resolver.find_matches(&graph, &res);
            for fqn_id in matches {
                if let Some(&idx) = graph.fqn_map().get(&fqn_id) {
                    let node = &topology[idx];
                    if let Some(loc) = &node.location {
                        let path_str = graph.symbols().resolve(&loc.path.0);
                        locations.push(SymbolLocation {
                            path: Arc::from(PathBuf::from(path_str)),
                            range: loc.range,
                            selection_range: loc.selection_range,
                        });
                    }
                }
            }
        }
        Ok(locations)
    }

    async fn find_implementations(&self, query: &SymbolQuery) -> ApiResult<Vec<SymbolLocation>> {
        self.hydrate_resolution_if_needed(&query.resolution).await?;

        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.find_implementations",
                    language: query.language.to_string(),
                });
            }
        };
        let graph = self.graph().await;
        let matches = resolver.find_implementations(&graph, &query.resolution);

        let topology = graph.topology();
        let mut locations = Vec::new();

        for fqn_id in matches {
            if let Some(&idx) = graph.fqn_map().get(&fqn_id) {
                let node = &topology[idx];
                if let Some(loc) = &node.location {
                    let path_str = graph.symbols().resolve(&loc.path.0);
                    locations.push(SymbolLocation {
                        path: Arc::from(PathBuf::from(path_str)),
                        range: loc.range,
                        selection_range: loc.selection_range,
                    });
                }
            }
        }
        Ok(locations)
    }
}

#[async_trait]
impl ReferenceAnalyzer for EngineHandle {
    async fn find_references(&self, query: &ReferenceQuery) -> ApiResult<Vec<SymbolLocation>> {
        self.hydrate_resolution_if_needed(&query.resolution).await?;

        let resolver = match self.get_semantic_resolver(query.language.clone()) {
            Some(r) => r,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.find_references",
                    language: query.language.to_string(),
                });
            }
        };
        let graph = self.graph().await;

        let mut matches = resolver.find_matches(&graph, &query.resolution);

        // If searching for a method/class, also include implementations as "matches"
        // for the purpose of filtering declarations.
        let impls = resolver.find_implementations(&graph, &query.resolution);
        matches.extend(impls);

        let match_indices: Vec<_> = matches
            .iter()
            .filter_map(|id| graph.fqn_map().get(id).copied())
            .collect();
        let conventions = (*self.naming_conventions()).clone();
        let discovery = DiscoveryEngine::new(&graph, conventions.clone());
        let candidate_paths = discovery.scout_references(&match_indices);

        let mut tasks = tokio::task::JoinSet::new();
        let shared_graph = Arc::new(graph);

        for path in candidate_paths {
            let handle = self.clone();
            let resolution = query.resolution.clone();
            let graph_snap = Arc::clone(&shared_graph);
            let conventions_clone = conventions.clone();

            tasks.spawn(async move {
                let (semantic, _file_lang) = match handle.get_services_for_path(&path) {
                    Some(x) => x,
                    None => return Vec::new(),
                };

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!("find_references failed to read {}: {}", path.display(), e);
                        return Vec::new();
                    }
                };

                let discovery = DiscoveryEngine::new(graph_snap.as_ref(), conventions_clone);

                let uri_str = format!("file://{}", path.display());
                let uri: lsp_types::Uri = match uri_str.parse() {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!("find_references failed to parse URI {}: {}", uri_str, e);
                        return Vec::new();
                    }
                };

                let locations = discovery.scan_file(semantic.as_ref(), &content, &resolution, &uri);

                locations
                    .into_iter()
                    .filter_map(|loc| {
                        let path_buf = url::Url::parse(&loc.uri.to_string())
                            .ok()
                            .and_then(|u| u.to_file_path().ok())?;
                        Some(SymbolLocation {
                            path: Arc::from(path_buf),
                            range: Range {
                                start_line: loc.range.start.line as usize,
                                start_col: loc.range.start.character as usize,
                                end_line: loc.range.end.line as usize,
                                end_col: loc.range.end.character as usize,
                            },
                            selection_range: None,
                        })
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

        // 4. Optional: Filter out declarations if requested
        if !query.include_declaration {
            let decl_locations: HashSet<_> = match_indices
                .iter()
                .filter_map(|&idx| {
                    let node = &shared_graph.topology()[idx];
                    let loc = node.location.as_ref()?;
                    let path = shared_graph.symbols().resolve(&loc.path.0);
                    let range = loc.selection_range.unwrap_or(loc.range);
                    Some((path.to_string(), range))
                })
                .collect();

            all_locations.retain(|loc| {
                let path_str = loc.path.to_string_lossy().to_string();
                !decl_locations.contains(&(path_str, loc.range))
            });
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
    async fn find_incoming_calls(&self, fqn: &str) -> ApiResult<Vec<CallHierarchyIncomingCall>> {
        self.hydrate_symbol_if_missing(fqn).await?;

        let graph = self.graph().await;
        let mut target_indices = graph.find_matches_by_fqn(fqn);

        if target_indices.is_empty() {
            return Ok(vec![]);
        }

        // 2. Identify unique languages from the found nodes to expand potential implementation targets
        let mut unique_langs = std::collections::HashSet::new();
        for &idx in &target_indices {
            let lang_symbol = graph.topology()[idx].lang.0;
            unique_langs.insert(graph.symbols().resolve(&lang_symbol).to_string());
        }

        // 3. For each language, find implementations to avoid them being counted
        // as callers when they are actually override sites.
        let resolution = SymbolResolution::Global(fqn.to_string());
        for lang_str in unique_langs {
            let language = Language::new(lang_str);
            if let Some(resolver) = self.get_semantic_resolver(language) {
                let impls = resolver.find_implementations(&graph, &resolution);
                for impl_id in impls {
                    if let Some(&node_idx) = graph.fqn_map().get(&impl_id) {
                        if !target_indices.contains(&node_idx) {
                            target_indices.push(node_idx);
                        }
                    }
                }
            }
        }

        // 1. Meso-level scouting for candidate files
        let conventions = (*self.naming_conventions()).clone();
        let discovery = DiscoveryEngine::new(&graph, conventions.clone());
        let candidate_paths = discovery.scout_references(&target_indices);

        // 2. Micro-level scanning
        let mut tasks = tokio::task::JoinSet::new();
        let shared_graph = Arc::new(graph.clone());
        let resolution = SymbolResolution::Global(fqn.to_string());

        for path in candidate_paths {
            let handle = self.clone();
            let res = resolution.clone();
            let graph_snap = Arc::clone(&shared_graph);
            let conventions_clone = conventions.clone();

            tasks.spawn(async move {
                let (semantic, _file_lang) = match handle.get_services_for_path(&path) {
                    Some(x) => x,
                    None => return vec![],
                };

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(e) => {
                        tracing::warn!(
                            "find_incoming_calls failed to read {}: {}",
                            path.display(),
                            e
                        );
                        return vec![];
                    }
                };

                let discovery = DiscoveryEngine::new(graph_snap.as_ref(), conventions_clone);
                let uri_str = format!("file://{}", path.display());
                let uri: lsp_types::Uri = match uri_str.parse() {
                    Ok(u) => u,
                    Err(e) => {
                        tracing::warn!(
                            "find_incoming_calls failed to parse URI {}: {}",
                            uri_str,
                            e
                        );
                        return vec![];
                    }
                };

                // Verification
                discovery.scan_file(semantic.as_ref(), &content, &res, &uri)
            });
        }

        let mut all_call_sites = Vec::new();
        while let Some(res) = tasks.join_next().await {
            if let Ok(locs) = res {
                all_call_sites.extend(locs);
            }
        }

        // 3. Meso-level: group call sites by caller method
        let mut caller_map: HashMap<petgraph::stable_graph::NodeIndex, Vec<Range>> = HashMap::new();

        for loc in all_call_sites {
            if let Some(path) = url::Url::parse(&loc.uri.to_string())
                .ok()
                .and_then(|u| u.to_file_path().ok())
            {
                if let Some(caller_idx) = graph.find_container_node_at(
                    &path,
                    loc.range.start.line as usize,
                    loc.range.start.character as usize,
                ) {
                    let node = &graph.topology()[caller_idx];
                    // Only include methods or constructors as callers
                    // AND avoid reflexive calls that are actually just the definition site
                    let is_reflexive = target_indices.contains(&caller_idx);

                    if matches!(node.kind(), NodeKind::Method | NodeKind::Constructor)
                        && !is_reflexive
                    {
                        caller_map.entry(caller_idx).or_default().push(Range {
                            start_line: loc.range.start.line as usize,
                            start_col: loc.range.start.character as usize,
                            end_line: loc.range.end.line as usize,
                            end_col: loc.range.end.character as usize,
                        });
                    }
                }
            }
        }

        let mut results = Vec::new();
        for (idx, ranges) in caller_map {
            let node = &graph.topology()[idx];
            let lang_str = graph.symbols().resolve(&node.lang.0);
            let convention = conventions
                .get(lang_str)
                .map(|c: &Arc<dyn naviscope_plugin::NamingConvention>| c.as_ref());
            let fqn_str = graph.render_fqn(node, convention);
            if let Some(display_node) = self
                .get_node_display(&fqn_str)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
            {
                results.push(CallHierarchyIncomingCall {
                    from: display_node,
                    from_ranges: ranges,
                });
            }
        }

        Ok(results)
    }

    async fn find_outgoing_calls(&self, fqn: &str) -> ApiResult<Vec<CallHierarchyOutgoingCall>> {
        self.hydrate_symbol_if_missing(fqn).await?;

        let graph = self.graph().await;
        let conventions = (*self.naming_conventions()).clone();
        let node_idx = match graph.find_node(fqn) {
            Some(idx) => idx,
            None => return Ok(vec![]),
        };

        let node = graph
            .get_node(node_idx)
            .ok_or_else(|| ApiError::Internal(format!("missing node for index {}", node_idx.index())))?;
        let symbols = graph.symbols();
        let path_str = node
            .path(symbols)
            .ok_or_else(|| ApiError::Internal("Node has no path".into()))?;
        let path = PathBuf::from(path_str);

        let range = node
            .range()
            .ok_or_else(|| ApiError::Internal("Node has no range".into()))?;

        let (semantic, _lang) = self
            .get_services_for_path(&path)
            .ok_or_else(|| ApiError::Internal("No services for file".into()))?;

        let content = fs::read_to_string(&path).map_err(|e| ApiError::Internal(e.to_string()))?;

        // Micro-level scanning: extract method body and find all calls
        let tree = semantic
            .parse(&content, None)
            .ok_or_else(|| ApiError::Internal("Failed to parse".into()))?;

        let mut outgoing_calls: HashMap<petgraph::stable_graph::NodeIndex, Vec<Range>> =
            HashMap::new();

        // Simple AST walk to find identifiers in range
        let mut stack = vec![tree.root_node()];

        while let Some(n) = stack.pop() {
            let n_range = n.range();
            if n_range.start_point.row > range.end_line {
                continue;
            }
            if n_range.end_point.row < range.start_line {
                // Not in range, but children might be
                for i in 0..n.child_count() {
                    if let Some(child) = n.child(i as u32) {
                        stack.push(child);
                    }
                }
                continue;
            }

            // Check if it's an identifier-like node
            if matches!(
                n.kind(),
                "identifier" | "method_invocation" | "call_expression"
            ) {
                let pos_ctx = PositionContext {
                    uri: format!("file://{}", path.display()),
                    line: n_range.start_point.row as u32,
                    char: n_range.start_point.column as u32,
                    content: Some(content.clone()),
                };

                if let Ok(Some(res)) = self.resolve_symbol_at(&pos_ctx).await {
                    let matches = semantic.find_matches(&graph, &res);
                    for fqn_id in matches {
                        if let Some(&m_idx) = graph.fqn_map().get(&fqn_id) {
                            let m_node = &graph.topology()[m_idx];
                            if matches!(m_node.kind(), NodeKind::Method | NodeKind::Constructor) {
                                outgoing_calls.entry(m_idx).or_default().push(Range {
                                    start_line: n_range.start_point.row,
                                    start_col: n_range.start_point.column,
                                    end_line: n_range.end_point.row,
                                    end_col: n_range.end_point.column,
                                });
                            }
                        }
                    }
                }
            }

            // Recurse children
            for i in 0..n.child_count() {
                if let Some(child) = n.child(i as u32) {
                    stack.push(child);
                }
            }
        }

        let mut results = Vec::new();
        for (idx, ranges) in outgoing_calls {
            let m_node = &graph.topology()[idx];
            let lang_str = graph.symbols().resolve(&m_node.lang.0);
            let convention = conventions
                .get(lang_str)
                .map(|c: &Arc<dyn naviscope_plugin::NamingConvention>| c.as_ref());
            let fqn_str = graph.render_fqn(m_node, convention);
            if let Some(display_node) = self
                .get_node_display(&fqn_str)
                .await
                .map_err(|e| ApiError::Internal(e.to_string()))?
            {
                results.push(CallHierarchyOutgoingCall {
                    to: display_node,
                    from_ranges: ranges,
                });
            }
        }

        Ok(results)
    }
}

#[async_trait]
impl SymbolInfoProvider for EngineHandle {
    async fn get_symbol_info(&self, fqn: &str) -> ApiResult<Option<DisplayGraphNode>> {
        self.hydrate_symbol_if_missing(fqn).await?;
        self.get_node_display(fqn)
            .await
            .map_err(|e| ApiError::Internal(e.to_string()))
    }

    async fn get_document_symbols(&self, uri: &str) -> ApiResult<Vec<DisplayGraphNode>> {
        let path = path_from_uri_like(uri);

        let (semantic, _lang) = match self.get_services_for_path(&path) {
            Some(x) => x,
            None => {
                return Err(ApiError::UnsupportedCapability {
                    capability: "semantic.get_document_symbols",
                    language: "unknown".to_string(),
                });
            }
        };

        let content = fs::read_to_string(&path).map_err(|e| ApiError::Internal(e.to_string()))?;

        let tree = semantic
            .parse(&content, None)
            .ok_or_else(|| ApiError::Internal("Failed to parse".into()))?;

        let symbols = semantic.extract_symbols(&tree, &content);

        Ok(symbols)
    }

    async fn get_language_for_document(&self, uri: &str) -> ApiResult<Option<Language>> {
        let path = path_from_uri_like(uri);

        Ok(self.get_language_for_path(&path))
    }
}

#[cfg(test)]
mod tests {
    use super::path_from_uri_like;
    use std::path::PathBuf;

    #[test]
    fn path_from_uri_like_handles_file_uri() {
        let p = path_from_uri_like("file:///tmp/naviscope_test.java");
        assert_eq!(p, PathBuf::from("/tmp/naviscope_test.java"));
    }

    #[test]
    fn path_from_uri_like_keeps_plain_path() {
        let p = path_from_uri_like("/tmp/naviscope_test.java");
        assert_eq!(p, PathBuf::from("/tmp/naviscope_test.java"));
    }
}
