use crate::LspServer;
use naviscope_core::engine::LanguageService;
use naviscope_core::model::graph::NodeKind;
use naviscope_core::query::CodeGraphLike;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn prepare_call_hierarchy(
    server: &LspServer,
    params: CallHierarchyPrepareParams,
) -> Result<Option<Vec<CallHierarchyItem>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };

    // EngineHandle::graph is async and returns CodeGraph
    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match engine.get_semantic_resolver(doc.language) {
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

    let mut items = Vec::new();
    let matches = {
        let resolver = match engine.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        resolver.find_matches(index, &resolution)
    };

    let topology = index.topology();

    for idx in matches {
        let node = &topology[idx];
        let kind = node.kind();
        if kind == NodeKind::Method || kind == NodeKind::Constructor {
            if let (Some(target_path), Some(range)) = (node.file_path(), node.range()) {
                let lsp_range = Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                };
                items.push(CallHierarchyItem {
                    name: node.name().to_string(),
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(node.fqn().to_string()),
                    uri: Url::from_file_path(target_path).unwrap(),
                    range: lsp_range,
                    selection_range: lsp_range,
                    data: Some(serde_json::to_value(node.fqn().to_string()).unwrap()),
                });
            }
        }
    }

    if !items.is_empty() {
        Ok(Some(items))
    } else {
        Ok(None)
    }
}

pub async fn incoming_calls(
    server: &LspServer,
    params: CallHierarchyIncomingCallsParams,
) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let fqn: String =
        serde_json::from_value(params.item.data.unwrap_or_default()).unwrap_or_default();
    if fqn.is_empty() {
        return Ok(None);
    }

    let engine_lock = server.engine.read().await;
    let engine = match &*engine_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;

    let target_idx = match index.fqn_map().get(fqn.as_str()) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    // 1. Precise resolution for the target
    // We already have the FQN, so we can construct a SymbolResolution::Global
    let resolution = naviscope_core::parser::SymbolResolution::Global(fqn.clone());

    let discovery = naviscope_core::analysis::discovery::DiscoveryEngine::new(index);
    let candidate_paths = discovery.scout_references(&[target_idx]);

    let mut call_map: std::collections::HashMap<petgraph::prelude::NodeIndex, Vec<Range>> =
        std::collections::HashMap::new();

    for path in candidate_paths {
        let uri = Url::from_file_path(&path).unwrap();
        let doc_data = if let Some(d) = server.documents.get(&uri) {
            Some((d.content.clone(), d.parser.clone()))
        } else {
            let content = std::fs::read_to_string(&path).ok();
            if let Some(content) = content {
                if let Some((parser, _)) = server.get_parser_and_lang_for_uri(&uri).await {
                    Some((content, parser))
                } else {
                    None
                }
            } else {
                None
            }
        };

        if let Some((content, parser)) = doc_data {
            let locations = discovery.scan_file(parser.as_ref(), &content, &resolution, &uri);
            for loc in locations {
                // Find containing node for each call site
                if let Some(container_idx) = index.find_container_node_at(
                    &path,
                    loc.range.start.line as usize,
                    loc.range.start.character as usize,
                ) {
                    // Skip if the occurrence is actually the definition of the target itself
                    if let Some(name_range) = index.topology()[target_idx].name_range() {
                        if name_range.start_line == loc.range.start.line as usize
                            && name_range.start_col == loc.range.start.character as usize
                        {
                            continue;
                        }
                    }
                    let container_node = &index.topology()[container_idx];
                    let kind = container_node.kind();
                    // Filter for methods/constructors
                    if kind == NodeKind::Method || kind == NodeKind::Constructor {
                        call_map.entry(container_idx).or_default().push(loc.range);
                    }
                }
            }
        }
    }

    let mut calls = Vec::new();
    let topology = index.topology();

    for (container_idx, ranges) in call_map {
        let node = &topology[container_idx];
        if let (Some(source_path), Some(range)) = (node.file_path(), node.range()) {
            let lsp_range = Range {
                start: Position::new(range.start_line as u32, range.start_col as u32),
                end: Position::new(range.end_line as u32, range.end_col as u32),
            };

            let from_item = CallHierarchyItem {
                name: node.name().to_string(),
                kind: SymbolKind::METHOD,
                tags: None,
                detail: Some(node.fqn().to_string()),
                uri: Url::from_file_path(source_path).unwrap(),
                range: lsp_range,
                selection_range: lsp_range,
                data: Some(serde_json::to_value(node.fqn().to_string()).unwrap()),
            };

            calls.push(CallHierarchyIncomingCall {
                from: from_item,
                from_ranges: ranges,
            });
        }
    }

    Ok(Some(calls))
}

pub async fn outgoing_calls(
    server: &LspServer,
    params: CallHierarchyOutgoingCallsParams,
) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let fqn: String =
        serde_json::from_value(params.item.data.unwrap_or_default()).unwrap_or_default();
    if fqn.is_empty() {
        return Ok(None);
    }

    let engine_lock = server.engine.read().await;
    let engine = match &*engine_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let graph = engine.graph().await;
    let index: &dyn CodeGraphLike = &graph;

    let node_idx = match index.fqn_map().get(fqn.as_str()) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    let node = &index.topology()[node_idx];
    let path = match node.file_path() {
        Some(p) => p,
        None => return Ok(None),
    };
    let uri = Url::from_file_path(path).unwrap();

    let doc_data = if let Some(d) = server.documents.get(&uri) {
        let resolver = engine.get_semantic_resolver(d.language);
        Some((d.content.clone(), d.tree.clone(), resolver))
    } else {
        let content = std::fs::read_to_string(path).ok();
        if let Some(content) = content {
            if let Some((parser, lang)) = server.get_parser_and_lang_for_uri(&uri).await {
                let tree = parser.parse(&content, None);
                let resolver = engine.get_semantic_resolver(lang);
                tree.map(|t| (content, t, resolver))
            } else {
                None
            }
        } else {
            None
        }
    };

    let (content, tree, resolver) = match doc_data {
        Some((c, t, Some(r))) => (c, t, r),
        _ => return Ok(None),
    };

    // Find all calls WITHIN the range of the source node
    let container_range = match node.range() {
        Some(r) => r,
        None => return Ok(None),
    };

    let mut call_map: std::collections::HashMap<petgraph::prelude::NodeIndex, Vec<Range>> =
        std::collections::HashMap::new();

    // Use a visitor or simple walk to find all identifier/method_invocation nodes within range
    let mut stack = vec![tree.root_node()];
    while let Some(n) = stack.pop() {
        let r = n.range();
        if r.start_point.row > container_range.end_line
            || r.end_point.row < container_range.start_line
        {
            continue;
        }

        // Check if this node is a call-like identifier
        if n.kind() == "identifier" || n.kind() == "type_identifier" {
            // Check if it's within the container range precisely
            if r.start_point.row >= container_range.start_line
                && r.end_point.row <= container_range.end_line
            {
                // Resolve it
                if let Some(res) = resolver.resolve_at(
                    &tree,
                    &content,
                    r.start_point.row,
                    r.start_point.column,
                    index,
                ) {
                    if let naviscope_core::parser::SymbolResolution::Global(target_fqn) = res {
                        if target_fqn != fqn {
                            // Avoid self-calls if desired, but hierarchy tests usually want them
                            let target_matches = resolver.find_matches(
                                index,
                                &naviscope_core::parser::SymbolResolution::Global(target_fqn),
                            );
                            for &t_idx in &target_matches {
                                let t_node = &index.topology()[t_idx];
                                if t_node.kind() == NodeKind::Method
                                    || t_node.kind() == NodeKind::Constructor
                                {
                                    let lsp_range = Range {
                                        start: Position::new(
                                            r.start_point.row as u32,
                                            r.start_point.column as u32,
                                        ),
                                        end: Position::new(
                                            r.end_point.row as u32,
                                            r.end_point.column as u32,
                                        ),
                                    };
                                    call_map.entry(t_idx).or_default().push(lsp_range);
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut cursor = n.walk();
        for child in n.children(&mut cursor) {
            stack.push(child);
        }
    }

    let mut calls = Vec::new();
    let topology = index.topology();

    for (target_idx, ranges) in call_map {
        let target_node = &topology[target_idx];
        if let (Some(target_path), Some(range)) = (target_node.file_path(), target_node.range()) {
            let lsp_range = Range {
                start: Position::new(range.start_line as u32, range.start_col as u32),
                end: Position::new(range.end_line as u32, range.end_col as u32),
            };

            let to_item = CallHierarchyItem {
                name: target_node.name().to_string(),
                kind: SymbolKind::METHOD,
                tags: None,
                detail: Some(target_node.fqn().to_string()),
                uri: Url::from_file_path(target_path).unwrap(),
                range: lsp_range,
                selection_range: lsp_range,
                data: Some(serde_json::to_value(target_node.fqn().to_string()).unwrap()),
            };

            calls.push(CallHierarchyOutgoingCall {
                to: to_item,
                from_ranges: ranges,
            });
        }
    }

    Ok(Some(calls))
}
