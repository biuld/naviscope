use super::EngineHandle;
use crate::analysis::discovery::DiscoveryEngine;
use crate::util::utf16_col_to_byte_col;
use async_trait::async_trait;
use naviscope_api::models::{
    CallHierarchyIncomingCall, CallHierarchyOutgoingCall, DocumentSymbol, Language,
    PositionContext, Range, ReferenceQuery, SymbolInfo, SymbolLocation, SymbolQuery,
    SymbolResolution,
};
use naviscope_api::semantic::{
    CallHierarchyAnalyzer, ReferenceAnalyzer, SemanticError, SemanticResult, SymbolInfoProvider,
    SymbolNavigator,
};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

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
        _fqn: &str,
    ) -> SemanticResult<Vec<CallHierarchyIncomingCall>> {
        // Placeholder
        Ok(vec![])
    }

    async fn find_outgoing_calls(
        &self,
        _fqn: &str,
    ) -> SemanticResult<Vec<CallHierarchyOutgoingCall>> {
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

    async fn get_document_symbols(&self, uri: &str) -> SemanticResult<Vec<DocumentSymbol>> {
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
            fs::read_to_string(&path).map_err(|e| SemanticError::Internal(e.to_string()))?;

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
