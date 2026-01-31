use crate::util::get_word_from_content;
use crate::LspServer;
use naviscope_core::engine::LanguageService;
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

    let graph = engine.graph().await;
    let resolver = match engine.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    tokio::task::spawn_blocking(move || {
        let index: &dyn CodeGraphLike = &graph;

        // 1. Precise resolution using Semantic Resolver
        let resolution = {
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
                        start_byte: 0,
                        end_byte: 0,
                        start_point: tree_sitter::Point::new(range.start_line, range.start_col),
                        end_point: tree_sitter::Point::new(range.end_line, range.end_col),
                    },
                    &doc.content,
                ),
            })));
        }

        let matches = resolver.find_matches(index, &resolution);
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
    })
    .await
    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?
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
    let resolver = match engine.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    tokio::task::spawn_blocking(move || {
        let index: &dyn CodeGraphLike = &graph;
        let topology = index.topology();

        // 1. Precise resolution using Semantic Resolver
        let resolution = {
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
    })
    .await
    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?
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
    let resolver = match engine.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
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
            &graph,
        ) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    if let SymbolResolution::Local(_, _) = resolution {
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
            let mut all_locations = Vec::new();
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
                            end: Position::new(r.end_point.row as u32, r.end_point.column as u32),
                        },
                    });
                }
            }
            return Ok(if all_locations.is_empty() {
                None
            } else {
                Some(all_locations)
            });
        }
    }

    let matches = resolver.find_matches(&graph, &resolution);
    let discovery = naviscope_core::analysis::discovery::DiscoveryEngine::new(&graph);
    let candidate_paths = discovery.scout_references(&matches);

    let mut join_set = tokio::task::JoinSet::<Vec<Location>>::new();

    for path in candidate_paths {
        let target_uri = Url::from_file_path(&path).unwrap();

        // 1. Check if the file is already open and parsed
        if let Some(d) = server.documents.get(&target_uri) {
            let content = d.content.clone();
            let parser = d.parser.clone();
            let resolution = resolution.clone();
            let target_uri = target_uri.clone();
            let graph = graph.clone();

            join_set.spawn(async move {
                let discovery = naviscope_core::analysis::discovery::DiscoveryEngine::new(&graph);
                discovery.scan_file(parser.as_ref(), &content, &resolution, &target_uri)
            });
            continue;
        }

        // 2. Identify the language and parser for the file
        let parser_data = server.get_parser_and_lang_for_uri(&target_uri).await;

        if let Some((parser, _)) = parser_data {
            let resolution = resolution.clone();
            let target_uri = target_uri.clone();
            let graph = graph.clone();

            join_set.spawn_blocking(move || {
                let content = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(_) => return vec![],
                };
                let discovery = naviscope_core::analysis::discovery::DiscoveryEngine::new(&graph);
                discovery.scan_file(parser.as_ref(), &content, &resolution, &target_uri)
            });
        }
    }

    let mut all_locations = Vec::new();
    while let Some(res) = join_set.join_next().await {
        if let Ok(locs) = res {
            all_locations.extend(locs);
        }
    }

    if !all_locations.is_empty() {
        // De-duplicate locations
        all_locations.sort_by(|a, b| {
            a.uri
                .as_str()
                .cmp(b.uri.as_str())
                .then(a.range.start.line.cmp(&b.range.start.line))
                .then(a.range.start.character.cmp(&b.range.start.character))
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
    let resolver = match engine.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };

    tokio::task::spawn_blocking(move || {
        let index: &dyn CodeGraphLike = &graph;
        let topology = index.topology();

        // 1. Precise resolution using Semantic Resolver
        let resolution = {
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
    })
    .await
    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?
}
