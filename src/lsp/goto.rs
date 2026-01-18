use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;
use crate::lsp::util::{uri_to_path, get_word_at};
use crate::model::lang::java::JavaElement;

pub async fn definition(backend: &Backend, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let path = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    
    // 1. Get the word under cursor
    let word = match get_word_at(&path, position.line as usize, position.character as usize) {
        Some(w) => w,
        None => {
            backend.client.log_message(MessageType::LOG, "No word found at cursor").await;
            return Ok(None);
        }
    };
    backend.client.log_message(MessageType::LOG, format!("Searching definition for word: '{}'", word)).await;

    // 2. Find all nodes with this name
    if let Some(nodes) = index.name_map.get(&word) {
        let mut locations = Vec::new();
        for &idx in nodes {
            let node = &index.graph[idx];
            if let (Some(target_path), Some(range)) = (node.file_path(), node.range()) {
                // Heuristic: If we are at a definition site, we don't want to just return ourselves
                // unless there are NO other definitions.
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
            backend.client.log_message(MessageType::LOG, format!("Found {} definitions", locations.len())).await;
            if locations.len() == 1 {
                return Ok(Some(GotoDefinitionResponse::Scalar(locations[0].clone())));
            } else {
                return Ok(Some(GotoDefinitionResponse::Array(locations)));
            }
        }
    }

    Ok(None)
}

pub async fn type_definition(backend: &Backend, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let path = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    
    // Heuristic: 
    // 1. If on a variable/field name, find its definition node, get type name, then find type's definition.
    // 2. If on a type name itself, just use the word to find definition.
    
    let word = match get_word_at(&path, position.line as usize, position.character as usize) {
        Some(w) => w,
        None => return Ok(None),
    };

    let mut type_names = Vec::new();

    // Check if we are on a node that has an explicit type
    if let Some(nodes) = index.name_map.get(&word) {
        for &idx in nodes {
            let node = &index.graph[idx];
            if let crate::model::graph::GraphNode::Code(crate::model::graph::CodeElement::Java { element, .. }) = node {
                match element {
                    JavaElement::Field(f) => type_names.push(f.type_name.clone()),
                    JavaElement::Method(m) => type_names.push(m.return_type.clone()),
                    _ => type_names.push(word.clone()), // Fallback to name itself
                }
            }
        }
    } else {
        type_names.push(word);
    }

    let mut locations = Vec::new();
    for name in type_names {
        if let Some(nodes) = index.name_map.get(&name) {
            for &idx in nodes {
                let target = &index.graph[idx];
                if let (Some(tp), Some(tr)) = (target.file_path(), target.range()) {
                    locations.push(Location {
                        uri: Url::from_file_path(tp).unwrap(),
                        range: Range {
                            start: Position::new(tr.start_line as u32, tr.start_col as u32),
                            end: Position::new(tr.end_line as u32, tr.end_col as u32),
                        },
                    });
                }
            }
        }
    }

    if !locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(locations)));
    }

    Ok(None)
}

pub async fn references(backend: &Backend, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;
    
    let path = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    
    let word = match get_word_at(&path, position.line as usize, position.character as usize) {
        Some(w) => w,
        None => return Ok(None),
    };
    backend.client.log_message(MessageType::LOG, format!("Finding references for word: '{}'", word)).await;

    let mut all_locations = Vec::new();

    // 1. Precise references from established edges
    if let Some(target_nodes) = index.name_map.get(&word) {
        for &node_idx in target_nodes {
            let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
                let edge = &index.graph[edge_idx];
                let source_node = &index.graph[neighbor_idx];
                if let (Some(source_path), Some(range)) = (source_node.file_path(), &edge.range) {
                    all_locations.push(Location {
                        uri: Url::from_file_path(source_path).unwrap(),
                        range: Range {
                            start: Position::new(range.start_line as u32, range.start_col as u32),
                            end: Position::new(range.end_line as u32, range.end_col as u32),
                        },
                    });
                }
            }
        }
    }

    // 2. Heuristic: Find all edges that point to any node named 'word'
    // This catches edges that were formed even if the exact FQN didn't match perfectly.
    let heuristic_refs = index.find_references_by_name(&word);
    for (source_node_idx, edge) in heuristic_refs {
        if let Some(source_node) = index.graph.node_weight(source_node_idx) {
            if let (Some(source_path), Some(range)) = (source_node.file_path(), &edge.range) {
                let loc = Location {
                    uri: Url::from_file_path(source_path).unwrap(),
                    range: Range {
                        start: Position::new(range.start_line as u32, range.start_col as u32),
                        end: Position::new(range.end_line as u32, range.end_col as u32),
                    },
                };
                if !all_locations.contains(&loc) {
                    all_locations.push(loc);
                }
            }
        }
    }

    if !all_locations.is_empty() {
        backend.client.log_message(MessageType::LOG, format!("Found {} references", all_locations.len())).await;
        return Ok(Some(all_locations));
    }

    Ok(None)
}

pub async fn implementation(backend: &Backend, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let path = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    let word = match get_word_at(&path, position.line as usize, position.character as usize) {
        Some(w) => w,
        None => return Ok(None),
    };

    if let Some(target_nodes) = index.name_map.get(&word) {
        let mut locations = Vec::new();
        for &node_idx in target_nodes {
            let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
                let edge = &index.graph[edge_idx];
                if edge.edge_type == crate::model::graph::EdgeType::Implements || edge.edge_type == crate::model::graph::EdgeType::InheritsFrom {
                    let source_node = &index.graph[neighbor_idx];
                    if let (Some(source_path), Some(range)) = (source_node.file_path(), source_node.range()) {
                        locations.push(Location {
                            uri: Url::from_file_path(source_path).unwrap(),
                            range: Range {
                                start: Position::new(range.start_line as u32, range.start_col as u32),
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
