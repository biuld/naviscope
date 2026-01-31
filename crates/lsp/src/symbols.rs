use crate::LspServer;
use naviscope_api::models::NodeKind;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn document_symbol(
    server: &LspServer,
    params: DocumentSymbolParams,
) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let api_symbols = match engine.get_document_symbols(uri.as_str()).await {
        Ok(s) => s,
        Err(_) => return Ok(None),
    };

    let lsp_symbols = convert_api_symbols(api_symbols);
    Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)))
}

fn convert_api_symbols(symbols: Vec<naviscope_api::models::DocumentSymbol>) -> Vec<DocumentSymbol> {
    symbols.into_iter().map(convert_api_symbol).collect()
}

fn convert_api_symbol(sym: naviscope_api::models::DocumentSymbol) -> DocumentSymbol {
    let range = Range {
        start: Position::new(sym.range.start_line as u32, sym.range.start_col as u32),
        end: Position::new(sym.range.end_line as u32, sym.range.end_col as u32),
    };
    let selection_range = Range {
        start: Position::new(
            sym.selection_range.start_line as u32,
            sym.selection_range.start_col as u32,
        ),
        end: Position::new(
            sym.selection_range.end_line as u32,
            sym.selection_range.end_col as u32,
        ),
    };

    #[allow(deprecated)]
    DocumentSymbol {
        name: sym.name,
        detail: None,
        kind: node_kind_to_symbol_kind(&sym.kind),
        tags: None,
        deprecated: None,
        range,
        selection_range,
        children: if sym.children.is_empty() {
            None
        } else {
            Some(convert_api_symbols(sym.children))
        },
    }
}

fn node_kind_to_symbol_kind(kind: &NodeKind) -> SymbolKind {
    match kind {
        NodeKind::Package => SymbolKind::PACKAGE,
        NodeKind::Module => SymbolKind::MODULE,
        NodeKind::Class => SymbolKind::CLASS,
        NodeKind::Interface => SymbolKind::INTERFACE,
        NodeKind::Enum => SymbolKind::ENUM,
        NodeKind::Annotation => SymbolKind::INTERFACE,
        NodeKind::Method => SymbolKind::METHOD,
        NodeKind::Constructor => SymbolKind::CONSTRUCTOR,
        NodeKind::Field => SymbolKind::FIELD,
        NodeKind::Variable => SymbolKind::VARIABLE,
        NodeKind::Project => SymbolKind::MODULE,
        NodeKind::Dependency => SymbolKind::MODULE,
        NodeKind::Task => SymbolKind::FUNCTION,
        NodeKind::Plugin => SymbolKind::MODULE,
        NodeKind::Custom(s) => match s.as_str() {
            "function" => SymbolKind::FUNCTION,
            "property" => SymbolKind::PROPERTY,
            _ => SymbolKind::VARIABLE,
        },
    }
}

pub async fn workspace_symbol(
    server: &LspServer,
    params: WorkspaceSymbolParams,
) -> Result<Option<Vec<SymbolInformation>>> {
    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    // Use engine's graph query for workspace symbols
    use naviscope_api::graph::GraphQuery;
    let query = GraphQuery::Find {
        pattern: params.query,
        kind: vec![],
        limit: 100,
    };

    let result = match engine.query(&query).await {
        Ok(r) => r,
        Err(_) => return Ok(None),
    };

    let symbols: Vec<SymbolInformation> = result
        .nodes
        .into_iter()
        .filter_map(|node| {
            let loc = node.location?;
            Some(SymbolInformation {
                name: node.name,
                kind: node_kind_to_symbol_kind(&node.kind),
                tags: None,
                #[allow(deprecated)]
                deprecated: None,
                location: Location {
                    uri: Url::from_file_path(loc.path).ok()?,
                    range: Range {
                        start: Position::new(
                            loc.range.start_line as u32,
                            loc.range.start_col as u32,
                        ),
                        end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
                    },
                },
                container_name: Some(node.id),
            })
        })
        .collect();

    Ok(Some(symbols))
}
