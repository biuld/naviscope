use super::JavaParser;
use crate::model::graph::NodeKind;
use crate::parser::LspParser;
use crate::parser::utils::{RawSymbol, build_symbol_hierarchy};
use std::collections::HashMap;
use tree_sitter::Tree;

impl LspParser for JavaParser {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language).ok()?;
        parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<crate::parser::DocumentSymbol> {
        // Only run Stage 1: Identification of entities.
        // We don't need full FQN resolution (naming) or relation resolution (Stage 3)
        // for building the local document symbol tree.
        let mut entities = Vec::new();
        let mut relations = Vec::new();
        let mut entities_map = HashMap::new();

        let all_matches = self.collect_matches(tree, source);

        // Pass None for package to keep FQNs local/relative during symbol extraction
        self.identify_entities(
            &all_matches,
            source,
            &None,
            &mut entities,
            &mut relations,
            &mut entities_map,
        );

        // Convert JavaEntity to RawSymbol for the tree builder
        let raw_symbols = entities
            .into_iter()
            .map(|e| {
                let kind = match e.element {
                    crate::model::lang::java::JavaElement::Class(_) => NodeKind::Class,
                    crate::model::lang::java::JavaElement::Interface(_) => NodeKind::Interface,
                    crate::model::lang::java::JavaElement::Enum(_) => NodeKind::Enum,
                    crate::model::lang::java::JavaElement::Annotation(_) => NodeKind::Annotation,
                    crate::model::lang::java::JavaElement::Method(ref m) => {
                        if m.is_constructor {
                            NodeKind::Constructor
                        } else {
                            NodeKind::Method
                        }
                    }
                    crate::model::lang::java::JavaElement::Field(_) => NodeKind::Field,
                    crate::model::lang::java::JavaElement::Package(_) => NodeKind::Package,
                };

                RawSymbol {
                    name: e.element.name().to_string(),
                    kind,
                    range: e
                        .element
                        .range()
                        .cloned()
                        .unwrap_or(crate::model::graph::Range {
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        }),
                    selection_range: e.element.name_range().cloned().unwrap_or(
                        crate::model::graph::Range {
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        },
                    ),
                    node: e.node,
                }
            })
            .collect();

        build_symbol_hierarchy(raw_symbols)
    }

    fn symbol_kind(&self, kind: &NodeKind) -> tower_lsp::lsp_types::SymbolKind {
        use tower_lsp::lsp_types::SymbolKind;
        match kind {
            NodeKind::Class => SymbolKind::CLASS,
            NodeKind::Interface => SymbolKind::INTERFACE,
            NodeKind::Enum => SymbolKind::ENUM,
            NodeKind::Annotation => SymbolKind::INTERFACE,
            NodeKind::Method => SymbolKind::METHOD,
            NodeKind::Constructor => SymbolKind::CONSTRUCTOR,
            NodeKind::Field => SymbolKind::FIELD,
            NodeKind::Package => SymbolKind::PACKAGE,
            _ => SymbolKind::VARIABLE,
        }
    }
}
