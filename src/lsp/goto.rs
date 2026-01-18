use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;
use crate::lsp::util::uri_to_path;
use std::path::PathBuf;

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
    
    // 1. Check if it's a reference/call (edge at position)
    if let Some((_from, to_idx, _edge)) = index.find_edge_at(&path, position.line as usize, position.character as usize) {
        if let Some(target_node) = index.graph.node_weight(to_idx) {
            if let (Some(target_path), Some(range)) = (target_node.file_path(), target_node.range()) {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: Url::from_file_path(target_path).unwrap(),
                    range: Range {
                        start: Position::new(range.start_line as u32, range.start_col as u32),
                        end: Position::new(range.end_line as u32, range.end_col as u32),
                    },
                })));
            }
        }
    }

    // 2. Check if it's a node definition (definition at position) - although GD usually means jumping *to* here,
    // sometimes GD on a name jumps to itself or we might want to handle it.
    
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
    
    if let Some(node_idx) = index.find_node_at(&path, position.line as usize, position.character as usize) {
        let node = &index.graph[node_idx];
        let mut type_fqn = None;

        if let crate::model::graph::GraphNode::Code(crate::model::graph::CodeElement::Java { element, .. }) = node {
            match element {
                JavaElement::Field(f) => {
                    type_fqn = Some(f.type_name.clone());
                }
                JavaElement::Method(m) => {
                    type_fqn = Some(m.return_type.clone());
                }
                _ => {}
            }
        }

        if let Some(fqn) = type_fqn {
            // Best effort resolution: check if it's a full FQN in map, or try to find a node with this name
            if let Some(&target_idx) = index.fqn_map.get(&fqn) {
                let target_node = &index.graph[target_idx];
                if let (Some(target_path), Some(range)) = (target_node.file_path(), target_node.range()) {
                    return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                        uri: Url::from_file_path(target_path).unwrap(),
                        range: Range {
                            start: Position::new(range.start_line as u32, range.start_col as u32),
                            end: Position::new(range.end_line as u32, range.end_col as u32),
                        },
                    })));
                }
            } else {
                // If not found as full FQN, it might be a short name. 
                // We could search for nodes ending with ".ShortName" or exactly "ShortName"
                for (name, &idx) in &index.fqn_map {
                    if name == &fqn || name.ends_with(&format!(".{}", fqn)) {
                        let target_node = &index.graph[idx];
                        if let (Some(target_path), Some(range)) = (target_node.file_path(), target_node.range()) {
                            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                uri: Url::from_file_path(target_path).unwrap(),
                                range: Range {
                                    start: Position::new(range.start_line as u32, range.start_col as u32),
                                    end: Position::new(range.end_line as u32, range.end_col as u32),
                                },
                            })));
                        }
                    }
                }
            }
        }
    }

    Ok(None)
}

pub async fn document_highlight(backend: &Backend, params: DocumentHighlightParams) -> Result<Option<Vec<DocumentHighlight>>> {
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
    
    if let Some(node_idx) = index.find_node_at(&path, position.line as usize, position.character as usize) {
        let mut highlights = Vec::new();
        let target_node = &index.graph[node_idx];

        // 1. The definition itself if in same file
        if let (Some(p), Some(range)) = (target_node.file_path(), target_node.range()) {
            if p == &path {
                highlights.push(DocumentHighlight {
                    range: Range {
                        start: Position::new(range.start_line as u32, range.start_col as u32),
                        end: Position::new(range.end_line as u32, range.end_col as u32),
                    },
                    kind: Some(DocumentHighlightKind::WRITE),
                });
            }
        }

        // 2. All references in same file (incoming edges with range)
        let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
        while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
            let edge = &index.graph[edge_idx];
            let source_node = &index.graph[neighbor_idx];
            
            if let (Some(source_path), Some(range)) = (source_node.file_path(), &edge.range) {
                if source_path == &path {
                    highlights.push(DocumentHighlight {
                        range: Range {
                            start: Position::new(range.start_line as u32, range.start_col as u32),
                            end: Position::new(range.end_line as u32, range.end_col as u32),
                        },
                        kind: Some(DocumentHighlightKind::READ),
                    });
                }
            }
        }
        
        return Ok(Some(highlights));
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
    
    if let Some(node_idx) = index.find_node_at(&path, position.line as usize, position.character as usize) {
        let mut locations = Vec::new();
        
        let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
        while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
            let edge = &index.graph[edge_idx];
            let source_node = &index.graph[neighbor_idx];
            
            if let (Some(source_path), Some(range)) = (source_node.file_path(), &edge.range) {
                locations.push(Location {
                    uri: Url::from_file_path(source_path).unwrap(),
                    range: Range {
                        start: Position::new(range.start_line as u32, range.start_col as u32),
                        end: Position::new(range.end_line as u32, range.end_col as u32),
                    },
                });
            }
        }
        
        return Ok(Some(locations));
    }

    Ok(None)
}

pub async fn implementation(backend: &Backend, params: GotoDefinitionParams) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let path: PathBuf = match uri_to_path(&uri) {
        Some(p) => p,
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match &*naviscope_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let index = naviscope.index();
    
    if let Some(node_idx) = index.find_node_at(&path, position.line as usize, position.character as usize) {
        let mut locations = Vec::new();
        
        let mut incoming = index.graph.neighbors_directed(node_idx, petgraph::Direction::Incoming).detach();
        while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
            let edge = &index.graph[edge_idx];
            let source_node = &index.graph[neighbor_idx];
            
            if edge.edge_type == crate::model::graph::EdgeType::Implements || edge.edge_type == crate::model::graph::EdgeType::InheritsFrom {
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
        
        return Ok(Some(GotoDefinitionResponse::Array(locations)));
    }

    Ok(None)
}
