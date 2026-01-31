use super::JavaParser;
use naviscope_core::model::NodeKind;
use naviscope_core::parser::LspParser;
use naviscope_core::parser::utils::{RawSymbol, build_symbol_hierarchy};
use std::collections::HashMap;
use tree_sitter::Tree;

impl LspParser for JavaParser {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language).ok()?;
        parser.parse(source, old_tree)
    }

    fn extract_symbols(
        &self,
        tree: &Tree,
        source: &str,
    ) -> Vec<naviscope_core::parser::DocumentSymbol> {
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
                    crate::model::JavaElement::Class(_) => NodeKind::Class,
                    crate::model::JavaElement::Interface(_) => NodeKind::Interface,
                    crate::model::JavaElement::Enum(_) => NodeKind::Enum,
                    crate::model::JavaElement::Annotation(_) => NodeKind::Annotation,
                    crate::model::JavaElement::Method(ref m) => {
                        if m.is_constructor {
                            NodeKind::Constructor
                        } else {
                            NodeKind::Method
                        }
                    }
                    crate::model::JavaElement::Field(_) => NodeKind::Field,
                    crate::model::JavaElement::Package(_) => NodeKind::Package,
                };

                RawSymbol {
                    name: e.name,
                    kind,
                    range: naviscope_core::parser::utils::range_from_ts(e.node.range()),
                    selection_range: e
                        .node
                        .child_by_field_name("name")
                        .map(|n| naviscope_core::parser::utils::range_from_ts(n.range()))
                        .unwrap_or_else(|| {
                            naviscope_core::parser::utils::range_from_ts(e.node.range())
                        }),
                    node: e.node,
                }
            })
            .collect();

        build_symbol_hierarchy(raw_symbols)
    }

    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind {
        use lsp_types::SymbolKind;
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

    fn find_occurrences(
        &self,
        source: &str,
        tree: &Tree,
        target: &naviscope_core::parser::SymbolResolution,
    ) -> Vec<naviscope_core::model::Range> {
        let mut ranges = Vec::new();
        let name = match target {
            naviscope_core::parser::SymbolResolution::Local(_, _) => {
                // Local resolution is usually handled by the caller or by a separate pass
                return Vec::new();
            }
            naviscope_core::parser::SymbolResolution::Precise(fqn, _)
            | naviscope_core::parser::SymbolResolution::Global(fqn) => {
                fqn.split('.').last().unwrap_or(fqn).to_string()
            }
        };

        if name.is_empty() {
            return ranges;
        }

        let query_str = format!(
            "((identifier) @ident (#eq? @ident \"{}\"))
             ((type_identifier) @ident (#eq? @ident \"{}\"))",
            name, name
        );

        if let Ok(query) = tree_sitter::Query::new(&tree.language(), &query_str) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
            use tree_sitter::StreamingIterator;
            while let Some(mat) = matches.next() {
                for cap in mat.captures {
                    let r = cap.node.range();
                    ranges.push(naviscope_core::model::Range {
                        start_line: r.start_point.row,
                        start_col: r.start_point.column,
                        end_line: r.end_point.row,
                        end_col: r.end_point.column,
                    });
                }
            }
        }
        ranges
    }
}
