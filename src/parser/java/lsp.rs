use crate::parser::LspParser;
use crate::parser::utils::{RawSymbol, build_symbol_hierarchy};
use tree_sitter::Tree;
use super::JavaParser;

impl LspParser for JavaParser {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language).ok()?;
        parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<crate::parser::DocumentSymbol> {
        // Use the native AST analyzer
        let model = self.analyze(tree, source);
        
        // Convert JavaEntity to RawSymbol for the tree builder
        let raw_symbols = model.entities
            .into_iter()
            .map(|e| {
                let kind = match e.element {
                    crate::model::lang::java::JavaElement::Class(_) => "class",
                    crate::model::lang::java::JavaElement::Interface(_) => "interface",
                    crate::model::lang::java::JavaElement::Enum(_) => "enum",
                    crate::model::lang::java::JavaElement::Annotation(_) => "annotation",
                    crate::model::lang::java::JavaElement::Method(ref m) => if m.is_constructor { "constructor" } else { "method" },
                    crate::model::lang::java::JavaElement::Field(_) => "field",
                };
                
                RawSymbol {
                    name: e.element.name().to_string(),
                    kind: kind.to_string(),
                    range: e.element.range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
                    selection_range: e.element.name_range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
                    node: e.node,
                }
            })
            .collect();

        build_symbol_hierarchy(raw_symbols)
    }

    fn symbol_kind(&self, kind: &str) -> tower_lsp::lsp_types::SymbolKind {
        use tower_lsp::lsp_types::SymbolKind;
        match kind {
            "class" => SymbolKind::CLASS,
            "interface" => SymbolKind::INTERFACE,
            "enum" => SymbolKind::ENUM,
            "annotation" => SymbolKind::INTERFACE,
            "method" => SymbolKind::METHOD,
            "constructor" => SymbolKind::CONSTRUCTOR,
            "field" => SymbolKind::FIELD,
            _ => SymbolKind::VARIABLE,
        }
    }
}
