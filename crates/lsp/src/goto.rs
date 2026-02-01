use crate::LspServer;
use naviscope_api::models::{PositionContext, SymbolQuery, SymbolResolution};
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn definition(
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // We need document content for PositionContext.
    // Ideally, PositionContext can take URI and Engine loads it, but for unsaved files we might want to pass content.
    // Our EngineHandle implementation reads from disk if content is None, or uses provided content.
    // LspServer has documents map.
    let content = server.documents.get(&uri).map(|d| d.content.clone());

    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content,
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };

    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(None), // Log error?
    };

    if let SymbolResolution::Local(range, _) = resolution {
        // Found declaration in the same file
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri,
            range: Range {
                start: Position::new(range.start_line as u32, range.start_col as u32),
                end: Position::new(range.end_line as u32, range.end_col as u32),
            },
        })));
    }

    // Determine language from file extension or document
    // For simplicity, we can get language from document map again or assume backend infers it.
    // But SymbolQuery needs language.
    // Let's get language from server documents if possible.
    let language = match server.documents.get(&uri).map(|d| d.language.clone()) {
        Some(l) => l,
        None => return Ok(None),
    };

    let query = SymbolQuery {
        resolution,
        language,
    };

    let definitions = match engine.find_definitions(&query).await {
        Ok(defs) => defs,
        Err(_) => return Ok(None),
    };

    let locations: Vec<Location> = definitions
        .into_iter()
        .map(|loc| Location {
            uri: Url::from_file_path(&*loc.path).unwrap(),
            range: Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            },
        })
        .collect();

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
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    // We can extract common logic (ctx creation) to a helper if needed later.
    let content = server.documents.get(&uri).map(|d| d.content.clone());
    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content,
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };

    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(None),
    };

    let language = match server.documents.get(&uri).map(|d| d.language.clone()) {
        Some(l) => l,
        None => return Ok(None),
    };

    let query = SymbolQuery {
        resolution,
        language,
    };

    let locations = match engine.find_type_definitions(&query).await {
        Ok(locs) => locs,
        Err(_) => return Ok(None),
    };

    let lsp_locations: Vec<Location> = locations
        .into_iter()
        .map(|loc| Location {
            uri: Url::from_file_path(&*loc.path).unwrap(),
            range: Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            },
        })
        .collect();

    if !lsp_locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(lsp_locations)));
    }

    Ok(None)
}

pub async fn references(
    server: &LspServer,
    params: ReferenceParams,
) -> Result<Option<Vec<Location>>> {
    let uri = params.text_document_position.text_document.uri;
    let position = params.text_document_position.position;

    let content = server.documents.get(&uri).map(|d| d.content.clone());
    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content: content.clone(), // Clone for ctx
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };

    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(None),
    };

    let language = match server.documents.get(&uri).map(|d| d.language.clone()) {
        Some(l) => l,
        None => return Ok(None),
    };

    // 1. Local textual references (Meso-level optimization not fully moved yet)
    // The previous implementation had a "smart" check for local variables to use AST search.
    // Ideally this logic should also be inside `engine.find_references`, but `find_references` is async and general.
    // For now we can keep the local textual search here if we want, OR move it to `find_references`.
    // Moving it to `find_references` is better for encapsulation.
    // The previous code check `SymbolResolution::Local`.
    // Let's rely on `engine.find_references` to handle it.
    // But wait, `EngineHandle::find_references` implementation we just wrote uses `DiscoveryEngine::scan_file`.
    // Does `DiscoveryEngine::scan_file` handle local variable textual matches efficiently?
    // It uses `parser.find_occurrences` if resolution is local?
    // Let's check `DiscoveryEngine::scan_file` implementation (which we didn't change).
    // Yes, `scan_file` calls `parser.find_occurrences` if it can.

    use naviscope_api::models::ReferenceQuery;
    let query = ReferenceQuery {
        resolution,
        language,
        include_declaration: params.context.include_declaration,
    };

    let locations = match engine.find_references(&query).await {
        Ok(locs) => locs,
        Err(_) => return Ok(None),
    };

    // If local references are found by engine, they are returned.
    // But `EngineHandle::find_references` spawns tasks for OTHER files found by scout.
    // Does it search the CURRENT file? passing `candidate_paths` from `scout_references`.
    // `scout_references` usually returns files containing the token. This includes the current file.
    // So the current file should be in the list and scanned.

    let lsp_locations: Vec<Location> = locations
        .into_iter()
        .map(|loc| Location {
            uri: Url::from_file_path(&*loc.path).unwrap(),
            range: Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            },
        })
        .collect();

    if !lsp_locations.is_empty() {
        return Ok(Some(lsp_locations));
    }

    Ok(None)
}

pub async fn implementation(
    server: &LspServer,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let content = server.documents.get(&uri).map(|d| d.content.clone());
    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content,
    };

    let engine_lock = server.engine.read().await;
    let engine = match &*engine_lock {
        Some(n) => n,
        None => return Ok(None),
    };

    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(r)) => r,
        Ok(None) => return Ok(None),
        Err(_) => return Ok(None),
    };

    let language = match server.documents.get(&uri).map(|d| d.language.clone()) {
        Some(l) => l,
        None => return Ok(None),
    };

    let query = SymbolQuery {
        resolution,
        language,
    };

    let locations = match engine.find_implementations(&query).await {
        Ok(locs) => locs,
        Err(_) => return Ok(None),
    };

    let lsp_locations: Vec<Location> = locations
        .into_iter()
        .map(|loc| Location {
            uri: Url::from_file_path(&*loc.path).unwrap(),
            range: Range {
                start: Position::new(loc.range.start_line as u32, loc.range.start_col as u32),
                end: Position::new(loc.range.end_line as u32, loc.range.end_col as u32),
            },
        })
        .collect();

    if !lsp_locations.is_empty() {
        return Ok(Some(GotoDefinitionResponse::Array(lsp_locations)));
    }

    Ok(None)
}
