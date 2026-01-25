use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use crate::lsp::LspServer;
use crate::model::graph::{GraphNode, CodeElement, BuildElement};
use crate::model::signature::TypeRef;
use crate::parser::SymbolResolution;

fn fmt_type(t: &TypeRef) -> String {
    match t {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(s) => s.split('.').last().unwrap_or(s).to_string(),
        TypeRef::Generic { base, args } => {
            let args_str = args.iter().map(fmt_type).collect::<Vec<_>>().join(", ");
            format!("{}<{}>", fmt_type(base), args_str)
        },
        TypeRef::Array { element, dimensions } => {
            format!("{}{}", fmt_type(element), "[]".repeat(*dimensions))
        },
        _ => "?".to_string(),
    }
}

fn get_node_signature(node: &GraphNode) -> Option<String> {
    match node {
        GraphNode::Code(code_el) => match code_el {
            CodeElement::Java { element, .. } => match element {
                crate::model::lang::java::JavaElement::Method(m) => {
                    let params_str = m.parameters.iter()
                        .map(|p| format!("{}", fmt_type(&p.type_ref)))
                        .collect::<Vec<_>>()
                        .join(", ");
                    let return_type_str = fmt_type(&m.return_type);
                    Some(format!("({}) -> {}", params_str, return_type_str))
                }
                crate::model::lang::java::JavaElement::Field(f) => {
                    Some(format!("{} {}", fmt_type(&f.type_ref), f.name))
                }
                _ => None,
            },
        },
        GraphNode::Build(build_el) => match build_el {
            BuildElement::Gradle { element, .. } => match element {
                crate::model::lang::gradle::GradleElement::Dependency(d) => {
                    let group = d.group.as_deref().unwrap_or("?");
                    let version = d.version.as_deref().unwrap_or("?");
                    Some(format!("{}:{}:{}", group, d.name, version))
                }
                _ => None,
            },
        },
    }
}

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
    let index = engine.graph();

    // 1. Precise resolution using Semantic Resolver
    let resolution = {
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
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
        let resolver = match server.resolver.get_semantic_resolver(doc.language) {
            Some(r) => r,
            None => return Ok(None),
        };
        resolver.find_matches(index, &resolution)
    };

    for &idx in &matches {
        let node = &index.topology[idx];
        if !hover_text.is_empty() {
            hover_text.push_str("\n\n---\n\n");
        }
        
        // Method/Field name as title
        hover_text.push_str(&format!("**{}** *{}*\n\n", node.name(), node.kind().to_string()));
        
        // Signature in code block
        if let Some(sig) = get_node_signature(node) {
            hover_text.push_str(&format!("```java\n{}\n```\n", sig));
        }
        
        // Metadata: FQN only
        hover_text.push_str(&format!("\n*`{}`*", node.fqn()));
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
}
