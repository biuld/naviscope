use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::LspServer;

pub async fn document_symbol(server: &LspServer, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    
    // 1. Try real-time AST-based symbols first (supports unsaved changes)
    if let Some(doc) = server.documents.get(&uri) {
        let symbols = doc.parser.extract_symbols(&doc.tree, &doc.content);
        if !symbols.is_empty() {
            let lsp_symbols = convert_symbols(symbols, doc.parser.as_ref());
            return Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)));
        }
    }

    Ok(None)
}

fn convert_symbols(symbols: Vec<crate::parser::DocumentSymbol>, parser: &dyn crate::parser::LspParser) -> Vec<DocumentSymbol> {
    symbols.into_iter().map(|s| convert_symbol(s, parser)).collect()
}

fn convert_symbol(sym: crate::parser::DocumentSymbol, parser: &dyn crate::parser::LspParser) -> DocumentSymbol {
    let range = Range {
        start: Position::new(sym.range.start_line as u32, sym.range.start_col as u32),
        end: Position::new(sym.range.end_line as u32, sym.range.end_col as u32),
    };
    let selection_range = Range {
        start: Position::new(sym.selection_range.start_line as u32, sym.selection_range.start_col as u32),
        end: Position::new(sym.selection_range.end_line as u32, sym.selection_range.end_col as u32),
    };

    #[allow(deprecated)]
    DocumentSymbol {
        name: sym.name,
        detail: None,
        kind: parser.symbol_kind(&sym.kind),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if sym.children.is_empty() {
            None
        } else {
            Some(convert_symbols(sym.children, parser))
        },
    }
}

pub async fn workspace_symbol(server: &LspServer, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
    let engine_lock = server.engine.read().await;
    let engine = match &*engine_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = engine.graph();
    let query = params.query.to_lowercase();
    let mut symbols = Vec::new();

    for node in index.topology.node_weights() {
        if node.name().to_lowercase().contains(&query) || node.fqn().to_string().to_lowercase().contains(&query) {
            if let (Some(path), Some(range)) = (node.file_path(), node.range()) {
                let kind = server.resolver.get_lsp_parser(node.language())
                    .map(|parser| parser.symbol_kind(&node.kind()))
                    .unwrap_or(SymbolKind::VARIABLE);

                #[allow(deprecated)]
                symbols.push(SymbolInformation {
                    name: node.name().to_string(),
                    kind,
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::from_file_path(path).unwrap(),
                        range: Range {
                            start: Position::new(range.start_line as u32, range.start_col as u32),
                            end: Position::new(range.end_line as u32, range.end_col as u32),
                        },
                    },
                    container_name: Some(node.fqn().to_string()),
                });
            }
        }
        if symbols.len() >= 100 {
            break;
        }
    }

    Ok(Some(symbols))
}
