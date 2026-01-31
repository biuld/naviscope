use crate::util::get_word_from_content;
use crate::LspServer;
use naviscope_core::parser::SymbolResolution;
use naviscope_core::query::CodeGraphLike;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tree_sitter::QueryCursor;

pub async fn definition(
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };

    // EngineHandle::graph is async and returns CodeGraph (cheap clone)
    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::util::utf16_col_to_byte_col(
            &doc.content,
            position.line as usize,
            position.character as usize,
        );
        match resolver.resolve_at(
            &doc.tree,
            &doc.content,
            position.line as usize,
            byte_col,
            index,
        ) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    if let SymbolResolution::Local(range, _) = resolution {
        // Found declaration in the same file
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: crate::util::to_lsp_range(
                tree_sitter::Range {
                    start_byte: 0, // Not used by to_lsp_range
                    end_byte: 0,
                    start_point: tree_sitter::Point::new(range.start_line, range.start_col),
                    end_point: tree_sitter::Point::new(range.end_line, range.end_col),
                },
                &doc.content,
            ),
        })));
    }

    let matches = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        resolver.find_matches(index, &resolution)
    };
    let mut locations = Vec::new();
    let topology = index.topology();

    for &node_idx in &matches {
        let node = &topology[node_idx];
        if let (Some(target_path), Some(range)) = (node.file_path(), node.range()) {
            locations.push(Location {
                uri: Url::from_file_path(target_path).unwrap(),
                range: Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                },
            });
        }
    }

    if !locations.is_empty() {
        if locations.len() == 1 {
            return Ok(Some(GotoDefinitionResponse::Scalar(locations[0].clone())));
        } else {
            return Ok(Some(GotoDefinitionResponse::Array(locations)));
        }
    }

    Ok(None)
}

pub async fn type_definition(
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };
    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;
    let topology = index.topology();

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::util::utf16_col_to_byte_col(
            &doc.content,
            position.line as usize,
            position.character as usize,
        );
        match resolver.resolve_at(
            &doc.tree,
            &doc.content,
            position.line as usize,
            byte_col,
            index,
        ) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    let resolver = match server.resolver.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    let type_resolutions = resolver.resolve_type_of(index, &resolution);

    let mut locations = Vec::new();
    for res in type_resolutions {
        let matches = resolver.find_matches(index, &res);
        for idx in matches {
            let target = &topology[idx];
            if let (Some(tp), Some(tr)) = (target.file_path(), target.range()) {
                let loc = Location {
                    uri: Url::from_file_path(tp).unwrap(),
                    range: Range {
                        start: Position::new(tr.start_line as u32, tr.start_col as u32),
                        end: Position::new(tr.end_line as u32, tr.end_col as u32),
                    },
                };
                if !locations.contains(&loc) {
                    locations.push(loc);
                }
            }
        }
    }

    if !locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(locations)));
    }

    Ok(None)
}

pub async fn references(
    server: &LspServer,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };
    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::util::utf16_col_to_byte_col(
            &doc.content,
            position.line as usize,
            position.character as usize,
        );
        match resolver.resolve_at(
            &doc.tree,
            &doc.content,
            position.line as usize,
            byte_col,
            index,
        ) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    let mut all_locations = Vec::new();

    match resolution {
        SymbolResolution::Local(_, _) => {
            // Find all occurrences of this name in current file's AST
            let word = get_word_from_content(
                &doc.content,
                position.line as usize,
                position.character as usize,
            )
            .unwrap_or_default();
            let query_str = format!("((identifier) @ident (#eq? @ident \"{}\"))", word);
            if let Ok(query) = tree_sitter::Query::new(&doc.tree.language(), &query_str) {
                let mut cursor = QueryCursor::new();
                let matches = cursor.matches(&query, doc.tree.root_node(), doc.content.as_bytes());
                use tree_sitter::StreamingIterator;
                let mut matches = matches;
                while let Some(mat) = matches.next() {
                    for cap in mat.captures {
                        let r = cap.node.range();
                        all_locations.push(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position::new(
                                    r.start_point.row as u32,
                                    r.start_point.column as u32,
                                ),
                                end: Position::new(
                                    r.end_point.row as u32,
                                    r.end_point.column as u32,
                                ),
                            },
                        });
                    }
                }
            }
        }
        _ => {
            let resolver = match server.resolver.get_semantic_resolver(doc.language) {
                Some(r) => r,
                None => return Ok(None),
            };

            let matches = resolver.find_matches(index, &resolution);

            let discovery = naviscope_core::analysis::discovery::DiscoveryEngine::new(index);
            let candidate_paths = discovery.scout_references(&matches);

            for path in candidate_paths {
                let target_uri = Url::from_file_path(&path).unwrap();
                let doc_data = if let Some(d) = server.documents.get(&target_uri) {
                    Some((d.content.clone(), d.parser.clone()))
                } else {
                    let content = std::fs::read_to_string(&path).ok();
                    if let Some(content) = content {
                        if let Some((parser, _)) = server.get_parser_and_lang_for_uri(&target_uri) {
                            Some((content, parser))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };

                if let Some((content, parser)) = doc_data {
                    all_locations.extend(discovery.scan_file(
                        parser.as_ref(),
                        &content,
                        &resolution,
                        &target_uri,
                    ));
                }
            }
        }
    }

    if !all_locations.is_empty() {
        // De-duplicate locations
        all_locations.sort_by_key(|l| {
            (
                l.uri.to_string(),
                l.range.start.line,
                l.range.start.character,
            )
        });
        all_locations.dedup();
        return Ok(Some(all_locations));
    }

    Ok(None)
}

pub async fn implementation(
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match &*engine_lock {
        Some(n) => n,
        None => return Ok(None),
    };
    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;
    let topology = index.topology();

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::util::utf16_col_to_byte_col(
            &doc.content,
            position.line as usize,
            position.character as usize,
        );
        match resolver.resolve_at(
            &doc.tree,
            &doc.content,
            position.line as usize,
            byte_col,
            index,
        ) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    let resolver = match server.resolver.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    let implementations = resolver.find_implementations(index, &resolution);
    let mut locations = Vec::new();

    for &node_idx in &implementations {
        let node = &topology[node_idx];
        if let (Some(source_path), Some(range)) = (node.file_path(), node.range()) {
            locations.push(Location {
                uri: Url::from_file_path(source_path).unwrap(),
                range: Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                },
            });
        }
    }

    if !locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(locations)));
    }

    Ok(None)
}
