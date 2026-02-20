use crate::LspServer;
use naviscope_api::models::{DisplayGraphNode, PositionContext, SymbolResolution};
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
    let content = server.documents.get(&uri).map(|d| d.content.clone());

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e.clone(),
        None => return Ok(None),
    };

    let ctx = PositionContext {
        uri: uri.to_string(),
        line: position.line,
        char: position.character,
        content,
    };

    // 1. Resolve the symbol at position
    let resolution = match engine.resolve_symbol_at(&ctx).await {
        Ok(Some(res)) => res,
        Ok(None) => return Ok(None),
        Err(e) => {
            tracing::warn!("hover resolve_symbol_at failed for {}: {}", uri, e);
            return Ok(None);
        }
    };

    let info = match &resolution {
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => {
            engine.get_symbol_info(fqn).await.ok().flatten()
        }
        SymbolResolution::Local(_, _) => None,
    };
    let hover_text = build_hover_text(&resolution, info.as_ref());

    if !hover_text.is_empty() {
        return Ok(Some(Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover_text)),
            range: None,
        }));
    }

    Ok(None)
}

fn build_hover_text(resolution: &SymbolResolution, info: Option<&DisplayGraphNode>) -> String {
    match resolution {
        SymbolResolution::Local(range, type_name) => {
            let mut hover_text = String::new();
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
            hover_text
        }
        SymbolResolution::Precise(fqn, intent) => build_symbol_hover(fqn, Some(*intent), info),
        SymbolResolution::Global(fqn) => build_symbol_hover(fqn, None, info),
    }
}

fn build_symbol_hover(
    fqn: &str,
    intent: Option<naviscope_api::models::SymbolIntent>,
    info: Option<&DisplayGraphNode>,
) -> String {
    let Some(info) = info else {
        return format_fallback_hover(fqn, intent);
    };

    let mut hover_text = String::new();
    let container_line = info.detail.clone().or_else(|| {
        fqn.split_once('#')
            .map(|(owner, _member)| format!("Declared in `{}`", owner))
    });

    if let Some(sig) = &info.signature {
        let lang_tag = &info.lang;
        hover_text.push_str(&format!("```{}\n{}\n```\n", lang_tag, sig));
    } else {
        hover_text.push_str(&format!("**{}** *{}*\n\n", info.name, info.kind.to_string()));
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
    hover_text
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_api::models::graph::{NodeKind, NodeSource, ResolutionStatus};
    use naviscope_api::models::symbol::Range;

    #[test]
    fn hover_local_contains_type_and_decl() {
        let text = build_hover_text(
            &SymbolResolution::Local(
                Range {
                    start_line: 3,
                    start_col: 8,
                    end_line: 3,
                    end_col: 12,
                },
                Some("List<String>".into()),
            ),
            None,
        );
        assert!(text.contains("Local variable"));
        assert!(text.contains("List<String>"));
        assert!(text.contains("Declared at `4:9`"));
    }

    #[test]
    fn hover_member_uses_signature_and_owner() {
        let info = DisplayGraphNode {
            id: "com.example.Service#getContext".into(),
            name: "getContext".into(),
            kind: NodeKind::Method,
            lang: "java".into(),
            source: NodeSource::Project,
            status: ResolutionStatus::Resolved,
            location: None,
            detail: None,
            signature: Some("SessionContext getContext()".into()),
            modifiers: vec![],
            children: None,
        };

        let text = build_hover_text(
            &SymbolResolution::Precise(
                "com.example.Service#getContext".into(),
                naviscope_api::models::SymbolIntent::Method,
            ),
            Some(&info),
        );
        assert!(text.contains("SessionContext getContext()"));
        assert!(text.contains("Declared in `com.example.Service`"));
    }

    #[test]
    fn hover_fallback_mentions_metadata_unavailable() {
        let text = build_hover_text(
            &SymbolResolution::Global("com.example.Missing#call".into()),
            None,
        );
        assert!(text.contains("Metadata unavailable"));
        assert!(text.contains("com.example.Missing"));
    }

    #[test]
    fn hover_external_marks_source() {
        let info = DisplayGraphNode {
            id: "java.util.List#size".into(),
            name: "size".into(),
            kind: NodeKind::Method,
            lang: "java".into(),
            source: NodeSource::External,
            status: ResolutionStatus::Resolved,
            location: None,
            detail: Some("Declared in `java.util.List`".into()),
            signature: Some("int size()".into()),
            modifiers: vec![],
            children: None,
        };

        let text = build_hover_text(
            &SymbolResolution::Global("java.util.List#size".into()),
            Some(&info),
        );
        assert!(text.contains("Source: external"));
    }
}
