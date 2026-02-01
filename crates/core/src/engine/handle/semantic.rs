use crate::analysis::discovery::DiscoveryEngine;
use crate::engine::EngineHandle;
use crate::util::utf16_col_to_byte_col;
use async_trait::async_trait;
use naviscope_api::models::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, DisplayGraphNode, Language, NodeKind,
    PositionContext, Range, ReferenceQuery, SymbolLocation, SymbolQuery, SymbolResolution,
};
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, SemanticError, SemanticResult, SymbolInfoProvider,
    SymbolNavigator,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

impl EngineHandle {
    fn node_to_display_node(
        &self,
        graph: &crate::engine::graph::CodeGraph,
        idx: petgraph::stable_graph::NodeIndex,
    ) -> DisplayGraphNode {
        let node = &graph.topology()[idx];
        let symbols = graph.symbols();
        let mut display_node = node.to_display(symbols);

        // Hydrate with language features if available
        let lang = node.language(symbols);
        if let Some(fp) = self.get_feature_provider(lang) {
            display_node.detail = fp.detail_view(&display_node);
            display_node.signature = fp.signature(&display_node);
            display_node.modifiers = fp.modifiers(&display_node);
        }

        display_node
    }

    pub(crate) fn hydrate_node(&self, mut node: DisplayGraphNode) -> DisplayGraphNode {
        let lang = Language::from(node.lang.as_str());
        if let Some(fp) = self.get_feature_provider(lang) {
            node.detail = fp.detail_view(&node);
            node.signature = fp.signature(&node);
            node.modifiers = fp.modifiers(&node);
        }

        if let Some(children) = node.children.take() {
            node.children = Some(
                children
                    .into_iter()
                    .map(|c| self.hydrate_node(c))
                    .collect(),
            );
        }

        node
    }
}

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
            fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?
        };

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

        let byte_col = utf16_col_to_byte_col(&content, ctx.line as usize, ctx.char as usize);

        let graph = self.graph().await;

        Ok(resolver.resolve_at(&tree, &content, ctx.line as usize, byte_col, &graph))
    }

    async fn find_highlights(&self, ctx: &PositionContext) -> SemanticResult<Vec<Range>> {
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
            fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?
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
            if let Some(loc) = &node.location {
                let path_str = graph.symbols().resolve(&loc.path.0);
                locations.push(SymbolLocation {
                    path: Arc::from(PathBuf::from(path_str)),
                    range: loc.range,
                    selection_range: loc.selection_range,
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
            if let Some(loc) = &node.location {
                let path_str = graph.symbols().resolve(&loc.path.0);
                locations.push(SymbolLocation {
                    path: Arc::from(PathBuf::from(path_str)),
                    range: loc.range,
                    selection_range: loc.selection_range,
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
        let discovery = DiscoveryEngine::new(&graph);
        let candidate_paths = discovery.scout_references(&matches);

        let mut tasks = tokio::task::JoinSet::new();
        let shared_graph = Arc::new(graph);

        for path in candidate_paths {
            let handle = self.clone();
            let resolution = query.resolution.clone();
            let graph_snap = Arc::clone(&shared_graph);

            tasks.spawn(async move {
                let (parser, file_lang) = match handle.get_parser_and_lang_for_path(&path) {
                    Some(x) => x,
                    None => return Vec::new(),
                };

                let file_resolver = match handle.get_semantic_resolver(file_lang) {
                    Some(r) => r,
                    None => return Vec::new(),
                };

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => return Vec::new(),
                };

                let discovery = DiscoveryEngine::new(graph_snap.as_ref());

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
                            path: Arc::from(path_buf),
                            range: Range {
                                start_line: loc.range.start.line as usize,
                                start_col: loc.range.start.character as usize,
                                end_line: loc.range.end.line as usize,
                                end_col: loc.range.end.character as usize,
                            },
                            selection_range: None,
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
        fqn: &str,
    ) -> SemanticResult<Vec<CallHierarchyIncomingCall>> {
        let graph = self.graph().await;
        let target_indices = graph.find_matches_by_fqn(fqn);
        if target_indices.is_empty() {
            return Ok(vec![]);
        }

        // 1. Meso-level scouting for candidate files
        let discovery = DiscoveryEngine::new(&graph);
        let candidate_paths = discovery.scout_references(&target_indices);

        // 2. Micro-level scanning
        let mut tasks = tokio::task::JoinSet::new();
        let shared_graph = Arc::new(graph.clone());
        let resolution = SymbolResolution::Global(fqn.to_string());

        for path in candidate_paths {
            let handle = self.clone();
            let res = resolution.clone();
            let graph_snap = Arc::clone(&shared_graph);

            tasks.spawn(async move {
                let (parser, file_lang) = match handle.get_parser_and_lang_for_path(&path) {
                    Some(x) => x,
                    None => return vec![],
                };

                let file_resolver = match handle.get_semantic_resolver(file_lang) {
                    Some(r) => r,
                    None => return vec![],
                };

                let content = match fs::read_to_string(&path) {
                    Ok(c) => c,
                    Err(_) => return vec![],
                };

                let discovery = DiscoveryEngine::new(graph_snap.as_ref());
                let uri_str = format!("file://{}", path.display());
                let uri = match url::Url::parse(&uri_str) {
                    Ok(u) => u,
                    Err(_) => return vec![],
                };

                // Verification
                discovery.scan_file(
                    parser.as_ref(),
                    file_resolver.as_ref(),
                    &content,
                    &res,
                    &uri,
                )
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
            if let Ok(path) = loc.uri.to_file_path() {
                if let Some(caller_idx) = graph.find_container_node_at(
                    &path,
                    loc.range.start.line as usize,
                    loc.range.start.character as usize,
                ) {
                    let node = &graph.topology()[caller_idx];
                    // Only include methods or constructors as callers
                    if matches!(node.kind(), NodeKind::Method | NodeKind::Constructor) {
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

        let results = caller_map
            .into_iter()
            .map(|(idx, ranges)| CallHierarchyIncomingCall {
                from: self.node_to_display_node(&graph, idx),
                from_ranges: ranges,
            })
            .collect();

        Ok(results)
    }

    async fn find_outgoing_calls(
        &self,
        fqn: &str,
    ) -> SemanticResult<Vec<CallHierarchyOutgoingCall>> {
        let graph = self.graph().await;
        let node_idx = match graph.find_node(fqn) {
            Some(idx) => idx,
            None => return Ok(vec![]),
        };

        let node = graph.get_node(node_idx).unwrap();
        let symbols = graph.symbols();
        let path_str = node
            .path(symbols)
            .ok_or_else(|| SemanticError::Internal("Node has no path".into()))?;
        let path = PathBuf::from(path_str);

        let range = node
            .range()
            .ok_or_else(|| SemanticError::Internal("Node has no range".into()))?;

        let (parser, lang) = self
            .get_parser_and_lang_for_path(&path)
            .ok_or_else(|| SemanticError::Internal("No parser for file".into()))?;
        let resolver = self
            .get_semantic_resolver(lang)
            .ok_or_else(|| SemanticError::Internal("No resolver for file".into()))?;

        let content =
            fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?;

        // Micro-level scanning: extract method body and find all calls
        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

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
                    stack.push(n.child(i as u32).unwrap());
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
                    let matches = resolver.find_matches(&graph, &res);
                    for &m_idx in &matches {
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

            // Recurse children
            for i in 0..n.child_count() {
                stack.push(n.child(i as u32).unwrap());
            }
        }

        let results = outgoing_calls
            .into_iter()
            .map(|(idx, ranges)| CallHierarchyOutgoingCall {
                to: self.node_to_display_node(&graph, idx),
                from_ranges: ranges,
            })
            .collect();

        Ok(results)
    }
}

#[async_trait]
impl SymbolInfoProvider for EngineHandle {
    async fn get_symbol_info(&self, fqn: &str) -> SemanticResult<Option<DisplayGraphNode>> {
        let graph = self.graph().await;
        let node_idx = match graph.find_node(fqn) {
            Some(idx) => idx,
            None => return Ok(None),
        };

        Ok(Some(self.node_to_display_node(&graph, node_idx)))
    }

    async fn get_document_symbols(&self, uri: &str) -> SemanticResult<Vec<DisplayGraphNode>> {
        let path = if uri.starts_with("file://") {
            PathBuf::from(uri.strip_prefix("file://").unwrap())
        } else {
            PathBuf::from(uri)
        };

        let (parser, lang) = match self.get_parser_and_lang_for_path(&path) {
            Some(x) => x,
            None => return Ok(vec![]),
        };

        let content =
            fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?;

        let tree = parser
            .parse(&content, None)
            .ok_or_else(|| SemanticError::Internal("Failed to parse".into()))?;

        let mut symbols = parser.extract_symbols(&tree, &content);

        // Hydrate symbols with path and language features
        let lang_str = lang.as_str().to_string();
        for sym in &mut symbols {
            sym.lang = lang_str.clone();
            if let Some(loc) = &mut sym.location {
                loc.path = path.to_string_lossy().to_string();
            }
        }

        Ok(symbols
            .into_iter()
            .map(|s| self.hydrate_node(s))
            .collect())
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
