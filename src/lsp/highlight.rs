use crate::lsp::LspServer;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

pub async fn highlight(
    server: &LspServer,
    params: DocumentHighlightParams,
) -> Result<Option<Vec<DocumentHighlight>>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;

    let doc = match server.documents.get(&uri) {
        Some(d) => d.clone(),
        None => return Ok(None),
    };

    // 1. Get the word under cursor precisely
    let word = crate::lsp::util::find_node_at(
        &doc.tree,
        &doc.content,
        position.line as usize,
        position.character as usize,
    )
    .and_then(|node| {
        if node.kind() == "identifier" || node.kind() == "type_identifier" {
            doc.content.get(node.byte_range())
        } else {
            None
        }
    })
    .map(|s| s.to_string());

    let word = match word {
        Some(w) => w,
        None => return Ok(None),
    };

    // 2. Find all occurrences of this word in the current file's AST
    let mut highlights = Vec::new();
    let language = doc.tree.language();
    let query_str = format!(
        "((identifier) @ident (#eq? @ident \"{}\")) ((type_identifier) @ident (#eq? @ident \"{}\"))",
        word, word
    );
    if let Ok(query) = tree_sitter::Query::new(&language, &query_str) {
        let mut cursor = tree_sitter::QueryCursor::new();
        let matches = cursor.matches(&query, doc.tree.root_node(), doc.content.as_bytes());

        use tree_sitter::StreamingIterator;
        let mut matches = matches;
        while let Some(mat) = matches.next() {
            for cap in mat.captures {
                let range = cap.node.range();
                highlights.push(DocumentHighlight {
                    range: crate::lsp::util::to_lsp_range(range, &doc.content),
                    kind: Some(DocumentHighlightKind::TEXT),
                });
            }
        }
    }

    if highlights.is_empty() {
        Ok(None)
    } else {
        Ok(Some(highlights))
    }
}
