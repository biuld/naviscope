use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::LspServer;
use crate::index::CodeGraph;
use crate::model::graph::EdgeType;
use std::collections::HashSet;
use std::path::Path;
use petgraph::stable_graph::NodeIndex;

pub async fn document_symbol(server: &LspServer, params: DocumentSymbolParams) -> Result<Option<DocumentSymbolResponse>> {
    let uri = params.text_document.uri;
    let path = match uri.to_file_path() {
        Ok(p) => p,
        Err(_) => return Ok(None),
    };

    // 1. Try to get symbols from the global graph first (semantic view)
    let engine_lock = server.engine.read().await;
    if let Some(engine) = &*engine_lock {
        let graph = engine.graph();
        let symbols = get_symbols_from_graph(graph, &path);
        if !symbols.is_empty() {
            if let Some((parser, _)) = server.get_parser_and_lang_for_uri(&uri) {
                let lsp_symbols = convert_symbols(symbols, parser.as_ref());
                return Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)));
            }
        }
    }
    drop(engine_lock);

    // 2. Fallback to real-time AST-based symbols first (supports unsaved changes)
    if let Some(doc) = server.documents.get(&uri) {
        let symbols = doc.parser.extract_symbols(&doc.tree, &doc.content);
        if !symbols.is_empty() {
            let lsp_symbols = convert_symbols(symbols, doc.parser.as_ref());
            return Ok(Some(DocumentSymbolResponse::Nested(lsp_symbols)));
        }
    }

    Ok(None)
}

fn get_symbols_from_graph(graph: &CodeGraph, path: &Path) -> Vec<crate::parser::DocumentSymbol> {
    let node_indices = match graph.path_to_nodes.get(path) {
        Some(indices) => indices,
        None => return vec![],
    };

    let node_set: HashSet<_> = node_indices.iter().cloned().collect();
    
    // Find roots: nodes in this file that don't have a parent in this same file
    let mut roots = Vec::new();
    for &idx in node_indices {
        let mut has_parent_in_file = false;
        let mut incoming = graph.topology.neighbors_directed(idx, petgraph::Direction::Incoming).detach();
        while let Some((edge_idx, parent_idx)) = incoming.next(&graph.topology) {
            if graph.topology[edge_idx].edge_type == EdgeType::Contains && node_set.contains(&parent_idx) {
                has_parent_in_file = true;
                break;
            }
        }
        if !has_parent_in_file {
            roots.push(idx);
        }
    }

    // Sort roots by line number
    roots.sort_by(|&a, &b| {
        let ra = graph.topology[a].range();
        let rb = graph.topology[b].range();
        match (ra, rb) {
            (Some(a), Some(b)) => a.start_line.cmp(&b.start_line),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    roots.into_iter()
        .map(|idx| build_symbol_tree(graph, idx, &node_set))
        .collect()
}

fn build_symbol_tree(graph: &CodeGraph, idx: NodeIndex, node_set: &HashSet<NodeIndex>) -> crate::parser::DocumentSymbol {
    let node = &graph.topology[idx];
    
    let mut children_indices = Vec::new();
    let mut outgoing = graph.topology.neighbors_directed(idx, petgraph::Direction::Outgoing).detach();
    while let Some((edge_idx, child_idx)) = outgoing.next(&graph.topology) {
        if graph.topology[edge_idx].edge_type == EdgeType::Contains && node_set.contains(&child_idx) {
            children_indices.push(child_idx);
        }
    }

    // Sort children by line number
    children_indices.sort_by(|&a, &b| {
        let ra = graph.topology[a].range();
        let rb = graph.topology[b].range();
        match (ra, rb) {
            (Some(a), Some(b)) => a.start_line.cmp(&b.start_line),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    let children = children_indices.into_iter()
        .map(|c_idx| build_symbol_tree(graph, c_idx, node_set))
        .collect();

    crate::parser::DocumentSymbol {
        name: node.name().to_string(),
        kind: node.kind(),
        range: node.range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
        selection_range: node.name_range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
        children,
    }
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
