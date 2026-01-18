use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;
use crate::lsp::util::uri_to_path;

pub async fn handle(backend: &Backend, params: HoverParams) -> Result<Option<Hover>> {
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
        let mut contents = Vec::new();
        
        contents.push(MarkedString::String(format!("**{}** ({})", node.name(), node.kind())));
        contents.push(MarkedString::String(format!("FQN: `{}`", node.fqn())));
        
        return Ok(Some(Hover {
            contents: HoverContents::Array(contents),
            range: None,
        }));
    }

    Ok(None)
}
