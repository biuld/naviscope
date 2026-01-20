use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;
use crate::model::graph::EdgeType;


pub async fn prepare_call_hierarchy(backend: &Backend, params: CallHierarchyPrepareParams) -> Result<Option<Vec<CallHierarchyItem>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match naviscope_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };
    let index = naviscope.index();

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match backend.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::lsp::util::utf16_col_to_byte_col(&doc.content, position.line as usize, position.character as usize);
        match resolver.resolve_at(&doc.tree, &doc.content, position.line as usize, byte_col, index) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    let mut items = Vec::new();
    let matches = {
        let resolver = match backend.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        resolver.find_matches(index, &resolution)
    };

    for idx in matches {
        let node = &index.graph[idx];
        let kind = node.kind();
        if kind == "method" || kind == "constructor" {
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

pub async fn incoming_calls(backend: &Backend, params: CallHierarchyIncomingCallsParams) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let fqn: String = serde_json::from_value(params.item.data.unwrap_or_default()).unwrap_or_default();
    if fqn.is_empty() { return Ok(None); }

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    let node_idx = match index.fqn_map.get(&fqn) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    let mut calls = Vec::new();
    let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
    
    while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
        let edge = &index.graph[edge_idx];
        if edge.edge_type == EdgeType::Calls {
            let source_node = &index.graph[neighbor_idx];
            if let (Some(source_path), Some(range)) = (source_node.file_path(), source_node.range()) {
                let lsp_range = Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                };

                let from_item = CallHierarchyItem {
                    name: source_node.name().to_string(),
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(source_node.fqn().to_string()),
                    uri: Url::from_file_path(source_path).unwrap(),
                    range: lsp_range,
                    selection_range: lsp_range,
                    data: Some(serde_json::to_value(source_node.fqn().to_string()).unwrap()),
                };

                let call_range = if let Some(r) = &edge.range {
                    Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    }
                } else {
                    lsp_range
                };

                calls.push(CallHierarchyIncomingCall {
                    from: from_item,
                    from_ranges: vec![call_range],
                });
            }
        }
    }

    Ok(Some(calls))
}

pub async fn outgoing_calls(backend: &Backend, params: CallHierarchyOutgoingCallsParams) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let fqn: String = serde_json::from_value(params.item.data.unwrap_or_default()).unwrap_or_default();
    if fqn.is_empty() { return Ok(None); }

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    let node_idx = match index.fqn_map.get(&fqn) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    let mut calls = Vec::new();
    let mut outgoing = index.graph.neighbors_directed(node_idx, petgraph::Direction::Outgoing).detach();
    
    while let Some((edge_idx, neighbor_idx)) = outgoing.next(&index.graph) {
        let edge = &index.graph[edge_idx];
        if edge.edge_type == EdgeType::Calls {
            let target_node = &index.graph[neighbor_idx];
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

                let call_range = if let Some(r) = &edge.range {
                    Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    }
                } else {
                    lsp_range
                };

                calls.push(CallHierarchyOutgoingCall {
                    to: to_item,
                    from_ranges: vec![call_range],
                });
            }
        }
    }

    Ok(Some(calls))
}
