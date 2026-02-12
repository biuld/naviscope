use crate::LspServer;
use naviscope_api::models::PositionContext;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

fn format_fallback_hover(
    fqn: &str,
    intent: Option<naviscope_api::models::SymbolIntent>,
) -> String {
    let mut text = String::new();
    let label = match intent {
        Some(naviscope_api::models::SymbolIntent::Type) => "Type",
        Some(naviscope_api::models::SymbolIntent::Method) => "Method",
        Some(naviscope_api::models::SymbolIntent::Field) => "Field",
        Some(naviscope_api::models::SymbolIntent::Variable) => "Variable",
        _ => "Symbol",
    };
    text.push_str(&format!("**{}**\n\n", label));

    if let Some((owner, _member)) = fqn.split_once('#') {
        text.push_str(&format!("Declared in `{}`\n\n", owner));
    } else if let Some((owner, _name)) = fqn.rsplit_once('.') {
        text.push_str(&format!("Defined in `{}`\n\n", owner));
    }

    text.push_str("*Metadata unavailable (symbol may not be indexed yet)*\n\n");
    text.push_str(&format!("*`{}`*", fqn));
    text
}

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
        naviscope_api::models::SymbolResolution::Local(range, type_name) => {
            hover_text.push_str("**Local variable**");
            if let Some(t) = type_name {
                hover_text.push_str(&format!(": `{}`", t));
            }
            hover_text.push_str("\n\n");
            hover_text.push_str(&format!(
                "Declared at `{}:{}`",
                range.start_line + 1,
                range.start_col + 1
            ));
            hover_text.push_str("\n\n");
            hover_text.push_str("*Scope: local*");
        }
        naviscope_api::models::SymbolResolution::Precise(fqn, intent) => {
            // Fetch detailed info for FQN
            if let Ok(Some(info)) = engine.get_symbol_info(&fqn).await {
                let detail = info.detail;
                let container_line = detail.or_else(|| {
                    fqn.split_once('#')
                        .map(|(owner, _member)| format!("Declared in `{}`", owner))
                });

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

                if let Some(container_line) = container_line {
                    hover_text.push_str(&container_line);
                    hover_text.push_str("\n\n");
                }

                match info.source {
                    naviscope_api::models::NodeSource::External => {
                        hover_text.push_str("*Source: external*\n\n");
                    }
                    naviscope_api::models::NodeSource::Builtin => {
                        hover_text.push_str("*Source: builtin*\n\n");
                    }
                    naviscope_api::models::NodeSource::Project => {}
                }

                hover_text.push_str(&format!("*`{}`*", fqn));
            } else {
                hover_text.push_str(&format_fallback_hover(&fqn, Some(intent)));
            }
        }
        naviscope_api::models::SymbolResolution::Global(fqn) => {
            if let Ok(Some(info)) = engine.get_symbol_info(&fqn).await {
                let detail = info.detail;
                let container_line = detail.or_else(|| {
                    fqn.split_once('#')
                        .map(|(owner, _member)| format!("Declared in `{}`", owner))
                });

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

                if let Some(container_line) = container_line {
                    hover_text.push_str(&container_line);
                    hover_text.push_str("\n\n");
                }

                match info.source {
                    naviscope_api::models::NodeSource::External => {
                        hover_text.push_str("*Source: external*\n\n");
                    }
                    naviscope_api::models::NodeSource::Builtin => {
                        hover_text.push_str("*Source: builtin*\n\n");
                    }
                    naviscope_api::models::NodeSource::Project => {}
                }

                hover_text.push_str(&format!("*`{}`*", fqn));
            } else {
                hover_text.push_str(&format_fallback_hover(&fqn, None));
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
