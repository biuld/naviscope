use crate::LspServer;
use naviscope_api::models::{PositionContext, SymbolResolution};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn prepare_call_hierarchy(
    server: &LspServer,
    params: CallHierarchyPrepareParams,
) -> Result<Option<Vec<CallHierarchyItem>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let content = server.documents.get(&uri).map(|d| d.content.clone());
    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content,
    };

    // 1. Resolve at position
    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(r)) => r,
        _ => return Ok(None),
    };

    let fqn = match resolution {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => fqn,
        SymbolResolution::Local(_, _) => return Ok(None),
    };

    // 2. Fetch symbol info to get name and kind
    let info = match engine.get_symbol_info(&fqn).await {
        Ok(Some(i)) => i,
        _ => return Ok(None),
    };

    let loc = info.location.as_ref().expect("Symbol must have location");
    let lsp_range = Range {
        start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
        end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
    };

    let item = CallHierarchyItem {
        name: info.name,
        kind: SymbolKind::METHOD, // Default for call hierarchy
        tags: None,
        detail: Some(fqn.clone()),
        uri: Url::from_file_path(&loc.path).unwrap(),
        range: lsp_range,
        selection_range: lsp_range,
        data: Some(serde_json::to_value(fqn).unwrap()),
    };

    Ok(Some(vec![item]))
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
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let calls = match engine.find_incoming_calls(&fqn).await {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let lsp_calls: Vec<CallHierarchyIncomingCall> = calls
        .into_iter()
        .map(|item| {
            let loc = item
                .from
                .location
                .as_ref()
                .expect("Caller must have location");
            let lsp_range = Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            };
            CallHierarchyIncomingCall {
                from: CallHierarchyItem {
                    name: item.from.name,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(item.from.id.clone()),
                    uri: Url::from_file_path(&loc.path).unwrap(),
                    range: lsp_range,
                    selection_range: lsp_range,
                    data: Some(serde_json::to_value(item.from.id).unwrap()),
                },
                from_ranges: item
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Some(lsp_calls))
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
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let calls = match engine.find_outgoing_calls(&fqn).await {
        Ok(c) => c,
        Err(_) => return Ok(None),
    };

    let lsp_calls: Vec<CallHierarchyOutgoingCall> = calls
        .into_iter()
        .map(|item| {
            let loc = item
                .to
                .location
                .as_ref()
                .expect("Callee must have location");
            let lsp_range = Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            };
            CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: item.to.name,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(item.to.id.clone()),
                    uri: Url::from_file_path(&loc.path).unwrap(),
                    range: lsp_range,
                    selection_range: lsp_range,
                    data: Some(serde_json::to_value(item.to.id).unwrap()),
                },
                from_ranges: item
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    })
                    .collect(),
            }
        })
        .collect();

    Ok(Some(lsp_calls))
}
