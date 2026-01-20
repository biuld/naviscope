use crate::lsp::Backend;
use crate::lsp::util::get_word_from_content;
use crate::model::lang::java::JavaElement;
use crate::parser::{SymbolIntent, SymbolResolution};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tree_sitter::QueryCursor;

pub async fn definition(
    backend: &Backend,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    // 1. Precise resolution using AST (Local Scope & Imports)
    let resolution = match doc.resolve_symbol(position.line as usize, position.character as usize) {
        Some(r) => r,
        None => return Ok(None),
    };

    if let SymbolResolution::Local(range) = resolution {
        // Found declaration in the same file
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: crate::lsp::util::to_lsp_range(
                tree_sitter::Range {
                    start_byte: 0, // Not used by to_lsp_range
                    end_byte: 0,
                    start_point: tree_sitter::Point::new(range.start_line, range.start_col),
                    end_point: tree_sitter::Point::new(range.end_line, range.end_col),
                },
                &doc.content,
            ),
        })));
    }

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match naviscope_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };
    let index = naviscope.index();

    let matches = index.find_matches(&resolution);
    let mut locations = Vec::new();

    for &node_idx in &matches {
        let node = &index.graph[node_idx];
        if let (Some(target_path), Some(range)) = (node.file_path(), node.range()) {
            locations.push(Location {
                uri: Url::from_file_path(target_path).unwrap(),
                range: Range {
                    start: Position::new(range.start_line as u32, range.start_col as u32),
                    end: Position::new(range.end_line as u32, range.end_col as u32),
                },
            });
        }
    }

    if !locations.is_empty() {
        if locations.len() == 1 {
            return Ok(Some(GotoDefinitionResponse::Scalar(locations[0].clone())));
        } else {
            return Ok(Some(GotoDefinitionResponse::Array(locations)));
        }
    }

    Ok(None)
}

pub async fn type_definition(
    backend: &Backend,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    // 1. Precise resolution using AST
    let resolution = match doc.resolve_symbol(position.line as usize, position.character as usize) {
        Some(r) => r,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match naviscope_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };
    let index = naviscope.index();

    let mut type_resolutions = Vec::new();

    match resolution {
        SymbolResolution::Local(_) => {
            // For local variables, we'd need to find their declared type name.
            let word = get_word_from_content(
                &doc.content,
                position.line as usize,
                position.character as usize,
            )
            .unwrap_or_default();
            if let Some(nodes) = index.name_map.get(&word) {
                for &idx in nodes {
                    let node = &index.graph[idx];
                    if let crate::model::graph::GraphNode::Code(
                        crate::model::graph::CodeElement::Java { element, .. },
                    ) = node
                    {
                        match element {
                            JavaElement::Field(f) => {
                                type_resolutions.push(SymbolResolution::Precise(f.type_name.clone(), SymbolIntent::Type))
                            }
                            JavaElement::Method(m) => {
                                type_resolutions.push(SymbolResolution::Precise(m.return_type.clone(), SymbolIntent::Type))
                            }
                            _ => {}
                        }
                    }
                }
            }
        }
        SymbolResolution::Precise(fqn, intent) => {
            type_resolutions.push(SymbolResolution::Precise(fqn, intent));
        }
        SymbolResolution::Heuristic(name, intent) => {
            if let Some(nodes) = index.name_map.get(&name) {
                for &idx in nodes {
                    let node = &index.graph[idx];
                    if let crate::model::graph::GraphNode::Code(
                        crate::model::graph::CodeElement::Java { element, .. },
                    ) = node
                    {
                        match element {
                            JavaElement::Field(f) => {
                                type_resolutions.push(SymbolResolution::Precise(f.type_name.clone(), SymbolIntent::Type))
                            }
                            JavaElement::Method(m) => {
                                type_resolutions.push(SymbolResolution::Precise(m.return_type.clone(), SymbolIntent::Type))
                            }
                            _ => type_resolutions.push(SymbolResolution::Heuristic(name.clone(), intent)),
                        }
                    }
                }
            } else {
                type_resolutions.push(SymbolResolution::Heuristic(name, intent));
            }
        }
    }

    let mut locations = Vec::new();
    for res in type_resolutions {
        let matches = index.find_matches(&res);
        for idx in matches {
            let target = &index.graph[idx];
            if let (Some(tp), Some(tr)) = (target.file_path(), target.range()) {
                let loc = Location {
                    uri: Url::from_file_path(tp).unwrap(),
                    range: Range {
                        start: Position::new(tr.start_line as u32, tr.start_col as u32),
                        end: Position::new(tr.end_line as u32, tr.end_col as u32),
                    },
                };
                if !locations.contains(&loc) {
                    locations.push(loc);
                }
            }
        }
    }

    if !locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(locations)));
    }

    Ok(None)
}

pub async fn references(
    backend: &Backend,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    // 1. Precise resolution using AST
    let resolution = match doc.resolve_symbol(position.line as usize, position.character as usize) {
        Some(r) => r,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match naviscope_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };
    let index = naviscope.index();

    let mut all_locations = Vec::new();

    match resolution {
        SymbolResolution::Local(_) => {
            // Find all occurrences of this name in current file's AST
            let word = get_word_from_content(
                &doc.content,
                position.line as usize,
                position.character as usize,
            )
            .unwrap_or_default();
            let query_str = format!("((identifier) @ident (#eq? @ident \"{}\"))", word);
            if let Ok(query) = tree_sitter::Query::new(&doc.tree.language(), &query_str) {
                let mut cursor = QueryCursor::new();
                let matches =
                    cursor.matches(&query, doc.tree.root_node(), doc.content.as_bytes());
                use tree_sitter::StreamingIterator;
                let mut matches = matches;
                while let Some(mat) = matches.next() {
                    for cap in mat.captures {
                        let r = cap.node.range();
                        all_locations.push(Location {
                            uri: uri.clone(),
                            range: Range {
                                start: Position::new(
                                    r.start_point.row as u32,
                                    r.start_point.column as u32,
                                ),
                                end: Position::new(
                                    r.end_point.row as u32,
                                    r.end_point.column as u32,
                                ),
                            },
                        });
                    }
                }
            }
        }
        _ => {
            let matches = index.find_matches(&resolution);
            for node_idx in matches {
                let mut incoming = index
                    .graph
                    .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                    .detach();
                while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
                    let edge = &index.graph[edge_idx];
                    let source_node = &index.graph[neighbor_idx];
                    if let (Some(source_path), Some(range)) =
                        (source_node.file_path(), &edge.range)
                    {
                        all_locations.push(Location {
                            uri: Url::from_file_path(source_path).unwrap(),
                            range: Range {
                                start: Position::new(
                                    range.start_line as u32,
                                    range.start_col as u32,
                                ),
                                end: Position::new(range.end_line as u32, range.end_col as u32),
                            },
                        });
                    }
                }
            }
            
            // Heuristic references by name
            if let SymbolResolution::Heuristic(name, _) | SymbolResolution::Precise(name, _) = resolution {
                let simple_name = name.split('.').last().unwrap_or(&name);
                let heuristic_refs = index.find_references_by_name(simple_name);
                for (source_node_idx, edge) in heuristic_refs {
                    if let Some(source_node) = index.graph.node_weight(source_node_idx) {
                        if let (Some(source_path), Some(range)) = (source_node.file_path(), &edge.range)
                        {
                            let loc = Location {
                                uri: Url::from_file_path(source_path).unwrap(),
                                range: Range {
                                    start: Position::new(
                                        range.start_line as u32,
                                        range.start_col as u32,
                                    ),
                                    end: Position::new(range.end_line as u32, range.end_col as u32),
                                },
                            };
                            if !all_locations.contains(&loc) {
                                all_locations.push(loc);
                            }
                        }
                    }
                }
            }
        }
    }

    if !all_locations.is_empty() {
        return Ok(Some(all_locations));
    }

    Ok(None)
}

pub async fn implementation(
    backend: &Backend,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };
    
    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    let word = match get_word_from_content(
        &doc.content,
        position.line as usize,
        position.character as usize,
    ) {
        Some(w) => w,
        None => return Ok(None),
    };

    if let Some(target_nodes) = index.name_map.get(&word) {
        let mut locations = Vec::new();
        for &node_idx in target_nodes {
            let mut incoming = index
                .graph
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
                let edge = &index.graph[edge_idx];
                if edge.edge_type == crate::model::graph::EdgeType::Implements
                    || edge.edge_type == crate::model::graph::EdgeType::InheritsFrom
                {
                    let source_node = &index.graph[neighbor_idx];
                    if let (Some(source_path), Some(range)) =
                        (source_node.file_path(), source_node.range())
                    {
                        locations.push(Location {
                            uri: Url::from_file_path(source_path).unwrap(),
                            range: Range {
                                start: Position::new(
                                    range.start_line as u32,
                                    range.start_col as u32,
                                ),
                                end: Position::new(range.end_line as u32, range.end_col as u32),
                            },
                        });
                    }
                }
            }
        }
        if !locations.is_empty() {
            return Ok(Some(GotoDefinitionResponse::Array(locations)));
        }
    }

    Ok(None)
}

