use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::Backend;

use crate::parser::SymbolResolution;

pub async fn hover(backend: &Backend, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let doc = match backend.document_states.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let naviscope_lock = backend.naviscope.read().await;
    let naviscope = match naviscope_lock.as_ref() {
        Some(n) => n,
        None => return Ok(None),
    };
    let index = naviscope.index();

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match backend.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        let byte_col = crate::lsp::util::utf16_col_to_byte_col(&doc.content, position.line as usize, position.character as usize);
        match resolver.resolve_at(&doc.tree, &doc.content, position.line as usize, byte_col, index) {
            Some(r) => r,
            None => return Ok(None),
        }
    };

    if let SymbolResolution::Local(_, _) = resolution {
        return Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String("**Local variable**".to_string())),
            range: None,
        }));
    }

    let mut hover_text = String::new();
    let matches = {
        let resolver = match backend.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        resolver.find_matches(index, &resolution)
    };

    for &idx in &matches {
        let node = &index.graph[idx];
        if !hover_text.is_empty() {
            hover_text.push_str("\n\n---\n\n");
        }
        hover_text.push_str(&format!("**{}** ({})\n\n", node.name(), node.kind()));
        hover_text.push_str(&format!("FQN: `{}`", node.fqn()));
    }

    if hover_text.is_empty() {
        if let SymbolResolution::Precise(fqn, _) = resolution {
            hover_text.push_str(&format!("**External Reference**\n\nFQN: `{}`", fqn));
        }
    }

    if !hover_text.is_empty() {
        return Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover_text)),
            range: None,
        }));
    }

    Ok(None)
}
