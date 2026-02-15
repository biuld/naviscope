use crate::LspServer;
use naviscope_api::models::{DisplayGraphNode, PositionContext, SymbolResolution};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

fn parse_hierarchy_fqn(data: Option<serde_json::Value>) -> Option<String> {
    match serde_json::from_value(data.unwrap_or_default()) {
        Ok(fqn) => Some(fqn),
        Err(e) => {
            tracing::warn!("failed to parse call hierarchy item data: {}", e);
            None
        }
    }
}

fn build_call_hierarchy_item(info: DisplayGraphNode, fqn: String) -> Option<CallHierarchyItem> {
    let loc = info.location.as_ref()?;
    let lsp_range = Range {
        start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
        end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
    };
    let uri = match Url::from_file_path(&loc.path) {
        Ok(uri) => uri,
        Err(()) => {
            tracing::warn!(
                "prepare_call_hierarchy failed to convert path to file URL: {:?}",
                loc.path
            );
            return None;
        }
    };
    let data = match serde_json::to_value(fqn.clone()) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!("prepare_call_hierarchy failed to serialize fqn: {}", e);
            None
        }
    };

    Some(CallHierarchyItem {
        name: info.name,
        kind: SymbolKind::METHOD, // Default for call hierarchy
        tags: None,
        detail: Some(fqn),
        uri,
        range: lsp_range,
        selection_range: lsp_range,
        data,
    })
}

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

    let Some(item) = build_call_hierarchy_item(info, fqn.clone()) else {
        tracing::warn!("prepare_call_hierarchy missing/invalid location for {}", fqn);
        return Ok(None);
    };

    Ok(Some(vec![item]))
}

pub async fn incoming_calls(
    server: &LspServer,
    params: CallHierarchyIncomingCallsParams,
) -> Result<Option<Vec<CallHierarchyIncomingCall>>> {
    let fqn = match parse_hierarchy_fqn(params.item.data) {
        Some(fqn) => fqn,
        None => return Ok(None),
    };
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
        Err(e) => {
            tracing::warn!("find_incoming_calls failed for {}: {}", fqn, e);
            return Ok(None);
        }
    };

    let lsp_calls: Vec<CallHierarchyIncomingCall> = calls
        .into_iter()
        .filter_map(|item| {
            let loc = item.from.location.as_ref()?;
            let lsp_range = Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            };
            let uri = match Url::from_file_path(&loc.path) {
                Ok(uri) => uri,
                Err(()) => {
                    tracing::warn!(
                        "incoming_calls failed to convert path to file URL: {:?}",
                        loc.path
                    );
                    return None;
                }
            };
            let data = serde_json::to_value(item.from.id.clone()).ok();
            Some(CallHierarchyIncomingCall {
                from: CallHierarchyItem {
                    name: item.from.name,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(item.from.id.clone()),
                    uri,
                    range: lsp_range,
                    selection_range: lsp_range,
                    data,
                },
                from_ranges: item
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    })
                    .collect(),
            })
        })
        .collect();

    Ok(Some(lsp_calls))
}

pub async fn outgoing_calls(
    server: &LspServer,
    params: CallHierarchyOutgoingCallsParams,
) -> Result<Option<Vec<CallHierarchyOutgoingCall>>> {
    let fqn = match parse_hierarchy_fqn(params.item.data) {
        Some(fqn) => fqn,
        None => return Ok(None),
    };
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
        Err(e) => {
            tracing::warn!("find_outgoing_calls failed for {}: {}", fqn, e);
            return Ok(None);
        }
    };

    let lsp_calls: Vec<CallHierarchyOutgoingCall> = calls
        .into_iter()
        .filter_map(|item| {
            let loc = item.to.location.as_ref()?;
            let lsp_range = Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            };
            let uri = match Url::from_file_path(&loc.path) {
                Ok(uri) => uri,
                Err(()) => {
                    tracing::warn!(
                        "outgoing_calls failed to convert path to file URL: {:?}",
                        loc.path
                    );
                    return None;
                }
            };
            let data = serde_json::to_value(item.to.id.clone()).ok();
            Some(CallHierarchyOutgoingCall {
                to: CallHierarchyItem {
                    name: item.to.name,
                    kind: SymbolKind::METHOD,
                    tags: None,
                    detail: Some(item.to.id.clone()),
                    uri,
                    range: lsp_range,
                    selection_range: lsp_range,
                    data,
                },
                from_ranges: item
                    .from_ranges
                    .into_iter()
                    .map(|r| Range {
                        start: Position::new(r.start_line as u32, r.start_col as u32),
                        end: Position::new(r.end_line as u32, r.end_col as u32),
                    })
                    .collect(),
            })
        })
        .collect();

    Ok(Some(lsp_calls))
}

#[cfg(test)]
mod tests {
    use super::{build_call_hierarchy_item, parse_hierarchy_fqn};
    use naviscope_api::models::graph::{DisplaySymbolLocation, NodeKind, NodeSource, ResolutionStatus};
    use naviscope_api::models::DisplayGraphNode;
    use naviscope_api::models::Range as ApiRange;

    #[test]
    fn parse_hierarchy_fqn_accepts_string_value() {
        let data = Some(serde_json::Value::String("com.example.A#m()".to_string()));
        let parsed = parse_hierarchy_fqn(data);
        assert_eq!(parsed.as_deref(), Some("com.example.A#m()"));
    }

    #[test]
    fn parse_hierarchy_fqn_rejects_non_string_value() {
        let data = Some(serde_json::json!({ "bad": true }));
        assert!(parse_hierarchy_fqn(data).is_none());
    }

    #[test]
    fn build_call_hierarchy_item_rejects_missing_location() {
        let info = DisplayGraphNode {
            id: "com.example.A#m()".to_string(),
            name: "m".to_string(),
            kind: NodeKind::Method,
            lang: "java".to_string(),
            source: NodeSource::Project,
            status: ResolutionStatus::Resolved,
            location: None,
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        };
        assert!(build_call_hierarchy_item(info, "com.example.A#m()".to_string()).is_none());
    }

    #[test]
    fn build_call_hierarchy_item_accepts_location() {
        let info = DisplayGraphNode {
            id: "com.example.A#m()".to_string(),
            name: "m".to_string(),
            kind: NodeKind::Method,
            lang: "java".to_string(),
            source: NodeSource::Project,
            status: ResolutionStatus::Resolved,
            location: Some(DisplaySymbolLocation {
                path: "/tmp/naviscope_hierarchy_test.java".to_string(),
                range: ApiRange {
                    start_line: 1,
                    start_col: 2,
                    end_line: 3,
                    end_col: 4,
                },
                selection_range: None,
            }),
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        };
        assert!(build_call_hierarchy_item(info, "com.example.A#m()".to_string()).is_some());
    }
}
