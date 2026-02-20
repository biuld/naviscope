use crate::LspServer;
use naviscope_api::models::PositionContext;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn highlight(
    server: &LspServer,
    params: DocumentHighlightParams,
) -> Result<Option<Vec<DocumentHighlight>>> {
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

    let highlights = match engine.find_highlights(&ctx).await {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!("find_highlights failed for {}: {}", uri, e);
            return Ok(None);
        }
    };

    let lsp_highlights: Vec<DocumentHighlight> = highlights
        .into_iter()
        .map(|range| DocumentHighlight {
            range: Range {
                start: Position::new(range.start_line as u32, range.start_col as u32),
                end: Position::new(range.end_line as u32, range.end_col as u32),
            },
            kind: Some(DocumentHighlightKind::TEXT),
        })
        .collect();

    if lsp_highlights.is_empty() {
        Ok(None)
    } else {
        Ok(Some(lsp_highlights))
    }
}
