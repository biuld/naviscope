use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;
use crate::lsp::util::uri_to_path;

pub async fn document_symbol(backend: &Backend, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    let path = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };
    
    // 1. Try real-time AST-based symbols first (supports unsaved changes)
    if let Some(doc) = backend.document_states.get(&uri) {
        let symbols = doc.extract_symbols();
        if !symbols.is_empty() {
            let lsp_symbols = convert_symbols(symbols);
            return Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)));
        }
    }

    // 2. Fallback to index-based symbols
    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    
    if let Some(nodes) = index.path_to_nodes.get(&path) {
        let mut symbols = Vec::new();
        for &idx in nodes {
            let node = &index.graph[idx];
            if let Some(range) = node.range() {
                let lsp_range = Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                };
                
                #[allow(deprecated)]
                symbols.push(DocumentSymbol {
                    name: node.name().to_string(),
                    detail: Some(node.fqn()),
                    kind: match node.kind() {
                        "class" => SymbolKind::CLASS,
                        "interface" => SymbolKind::INTERFACE,
                        "enum" => SymbolKind::ENUM,
                        "method" => SymbolKind::METHOD,
                        "field" => SymbolKind::FIELD,
                        _ => SymbolKind::VARIABLE,
                    },
                    tags: None,
                    deprecated: None,
                    range: lsp_range,
                    selection_range: lsp_range,
                    children: None,
                });
            }
        }
        return Ok(Some(DocumentSymbolResponse::Nested(symbols)));
    }

    Ok(None)
}

fn convert_symbols(symbols: Vec<crate::parser::DocumentSymbol>) -> Vec<DocumentSymbol> {
    symbols.into_iter().map(convert_symbol).collect()
}

fn convert_symbol(sym: crate::parser::DocumentSymbol) -> DocumentSymbol {
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
        kind: match sym.kind.as_str() {
            "class" => SymbolKind::CLASS,
            "interface" => SymbolKind::INTERFACE,
            "method" => SymbolKind::METHOD,
            "constructor" => SymbolKind::CONSTRUCTOR,
            "field" => SymbolKind::FIELD,
            _ => SymbolKind::VARIABLE,
        },
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if sym.children.is_empty() {
            None
        } else {
            Some(convert_symbols(sym.children))
        },
    }
}

pub async fn workspace_symbol(backend: &Backend, params: WorkspaceSymbolParams) -> Result<Option<Vec<SymbolInformation>>> {
    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    let query = params.query.to_lowercase();
    let mut symbols = Vec::new();

    for node in index.graph.node_weights() {
        if node.name().to_lowercase().contains(&query) || node.fqn().to_lowercase().contains(&query) {
            if let (Some(path), Some(range)) = (node.file_path(), node.range()) {
                #[allow(deprecated)]
                symbols.push(SymbolInformation {
                    name: node.name().to_string(),
                    kind: match node.kind() {
                        "class" => SymbolKind::CLASS,
                        "interface" => SymbolKind::INTERFACE,
                        "enum" => SymbolKind::ENUM,
                        "method" => SymbolKind::METHOD,
                        "field" => SymbolKind::FIELD,
                        _ => SymbolKind::VARIABLE,
                    },
                    tags: None,
                    deprecated: None,
                    location: Location {
                        uri: Url::from_file_path(path).unwrap(),
                        range: Range {
                            start: Position::new(range.start_line as u32, range.start_col as u32),
                            end: Position::new(range.end_line as u32, range.end_col as u32),
                        },
                    },
                    container_name: Some(node.fqn()),
                });
            }
        }
        if symbols.len() >= 100 {
            break;
        }
    }

    Ok(Some(symbols))
}
