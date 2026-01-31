use crate::LspServer;
use naviscope_core::engine::LanguageService;
use naviscope_core::parser::SymbolResolution;
use naviscope_core::query::CodeGraphLike;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn hover(server: &LspServer, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    let engine_lock = server.engine.read().await;
    let engine = match engine_lock.as_ref() {
        Some(e) => e,
        None => return Ok(None),
    };

    let graph = engine.graph().await;
    let resolver = match engine.get_semantic_resolver(doc.language) {
        Some(r) => r,
        None => return Ok(None),
    };
    let feature_provider = engine.get_feature_provider(doc.language);

    tokio::task::spawn_blocking(move || {
        let index: &dyn CodeGraphLike = &graph;
        let topology = index.topology();

        // 1. Precise resolution using Semantic Resolver
        let byte_col = crate::util::utf16_col_to_byte_col(
            &doc.content,
            position.line as usize,
            position.character as usize,
        );
        let resolution = match resolver.resolve_at(
            &doc.tree,
            &doc.content,
            position.line as usize,
            byte_col,
            index,
        ) {
            Some(r) => r,
            None => return Ok(None),
        };

        if let SymbolResolution::Local(_, _) = resolution {
            return Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(
                    "**Local variable**".to_string(),
                )),
                range: None,
            }));
        }

        let mut hover_text = String::new();
        let matches = resolver.find_matches(index, &resolution);

        for &idx in &matches {
            let node = &topology[idx];
            if !hover_text.is_empty() {
                hover_text.push_str("\n\n---\n\n");
            }

            // Method/Field name as title
            hover_text.push_str(&format!(
                "**{}** *{}*\n\n",
                node.name(),
                node.kind().to_string()
            ));

            // Signature in code block (use feature provider if available)
            if let Some(provider) = &feature_provider {
                if let Some(sig) = provider.signature(node) {
                    hover_text.push_str(&format!("```java\n{}\n```\n", sig));
                }
            }

            // Metadata: FQN only
            hover_text.push_str(&format!("\n*`{}`*", node.id));
        }

        if hover_text.is_empty() {
            if let SymbolResolution::Precise(fqn, _) = resolution {
                hover_text.push_str(&format!("**External Reference**\n\n*`{}`*", fqn));
            }
        }

        if !hover_text.is_empty() {
            return Ok(Some(Hover {
                contents: HoverContents::Scalar(MarkedString::String(hover_text)),
                range: None,
            }));
        }

        Ok(None)
    })
    .await
    .map_err(|_| tower_lsp::jsonrpc::Error::internal_error())?
}
