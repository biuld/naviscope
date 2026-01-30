use crate::lsp::LspServer;
use crate::model::graph::{EdgeType, NodeKind};
use crate::query::CodeGraphLike;
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
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::lsp::util::utf16_col_to_byte_col(
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
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
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

    let node_idx = match index.fqn_map().get(&fqn) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    let mut calls = Vec::new();
    let topology = index.topology();
    let mut incoming = topology
        .neighbors_directed(node_idx, petgraph::Direction::Incoming)
        .detach();

    while let Some((edge_idx, neighbor_idx)) = incoming.next(topology) {
        let edge = &topology[edge_idx];
        if edge.edge_type == EdgeType::Calls {
            let source_node = &topology[neighbor_idx];
            if let (Some(source_path), Some(range)) = (source_node.file_path(), source_node.range())
            {
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

                let call_range = lsp_range;

                calls.push(CallHierarchyIncomingCall {
                    from: from_item,
                    from_ranges: vec![call_range],
                });
            }
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

    let node_idx = match index.fqn_map().get(&fqn) {
        Some(&idx) => idx,
        None => return Ok(None),
    };

    let mut calls = Vec::new();
    let topology = index.topology();
    let mut outgoing = topology
        .neighbors_directed(node_idx, petgraph::Direction::Outgoing)
        .detach();

    while let Some((edge_idx, neighbor_idx)) = outgoing.next(topology) {
        let edge = &topology[edge_idx];
        if edge.edge_type == EdgeType::Calls {
            let target_node = &topology[neighbor_idx];
            if let (Some(target_path), Some(range)) = (target_node.file_path(), target_node.range())
            {
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

                let call_range = lsp_range;

                calls.push(CallHierarchyOutgoingCall {
                    to: to_item,
                    from_ranges: vec![call_range],
                });
            }
        }
    }

    Ok(Some(calls))
}
