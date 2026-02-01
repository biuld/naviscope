use crate::LspServer;
use naviscope_api::models::PositionContext;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn hover(server: &LspServer, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content: None, // Engine will read from disk if needed
    };

    // 1. Resolve the symbol at position
    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(res)) => res,
        Ok(None) => return Ok(None),
        Err(e) => {
            return Err(tower_lsp::jsonrpc::Error::invalid_params(format!(
                "Resolution error: {}",
                e
            )));
        }
    };

    // 2. Map resolution to hover text
    let mut hover_text = String::new();

    match resolution {
        naviscope_api::models::SymbolResolution::Local(_, type_name) => {
            hover_text.push_str("**Local variable**");
            if let Some(t) = type_name {
                hover_text.push_str(&format!(": `{}`", t));
            }
        }
        naviscope_api::models::SymbolResolution::Precise(fqn, _)
        | naviscope_api::models::SymbolResolution::Global(fqn) => {
            // Fetch detailed info for FQN
            if let Ok(Some(info)) = engine.get_symbol_info(&fqn).await {
                if let Some(sig) = info.signature {
                    let lang_tag = info.lang;
                    hover_text.push_str(&format!("```{}\n{}\n```\n", lang_tag, sig));
                } else {
                    hover_text.push_str(&format!(
                        "**{}** *{}*\n\n",
                        info.name,
                        info.kind.to_string()
                    ));
                }

                if let Some(detail) = info.detail {
                    hover_text.push_str(&detail);
                    hover_text.push_str("\n\n");
                }

                hover_text.push_str(&format!("*`{}`*", fqn));
            } else {
                // Fallback to FQN only
                hover_text.push_str(&format!("**Symbol**\n\n*`{}`*", fqn));
            }
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
