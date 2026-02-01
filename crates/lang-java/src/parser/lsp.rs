use super::JavaParser;
use naviscope_core::ingest::parser::LspParser;
use naviscope_core::ingest::parser::utils::{RawSymbol, build_symbol_hierarchy};
use naviscope_core::model::NodeKind;
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
    ) -> Vec<naviscope_core::model::DisplayGraphNode> {
        self.extract_symbols(tree, source)
    }

    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind {
        self.symbol_kind(kind)
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &Tree,
        target: &naviscope_core::ingest::parser::SymbolResolution,
    ) -> Vec<naviscope_core::model::Range> {
        self.find_occurrences(source, tree, target)
    }
}

impl JavaParser {
    pub fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language).ok()?;
        parser.parse(source, old_tree)
    }

    pub fn extract_symbols(
        &self,
        tree: &Tree,
        source: &str,
    ) -> Vec<naviscope_core::model::DisplayGraphNode> {
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
                    crate::model::JavaIndexMetadata::Class { .. } => NodeKind::Class,
                    crate::model::JavaIndexMetadata::Interface { .. } => NodeKind::Interface,
                    crate::model::JavaIndexMetadata::Enum { .. } => NodeKind::Enum,
                    crate::model::JavaIndexMetadata::Annotation { .. } => NodeKind::Annotation,
                    crate::model::JavaIndexMetadata::Method { is_constructor, .. } => {
                        if is_constructor {
                            NodeKind::Constructor
                        } else {
                            NodeKind::Method
                        }
                    }
                    crate::model::JavaIndexMetadata::Field { .. } => NodeKind::Field,
                    crate::model::JavaIndexMetadata::Package => NodeKind::Package,
                };

                RawSymbol {
                    name: e.name,
                    kind,
                    range: naviscope_core::ingest::parser::utils::range_from_ts(e.node.range()),
                    selection_range: e
                        .node
                        .child_by_field_name("name")
                        .map(|n| naviscope_core::ingest::parser::utils::range_from_ts(n.range()))
                        .unwrap_or_else(|| {
                            naviscope_core::ingest::parser::utils::range_from_ts(e.node.range())
                        }),
                    node: e.node,
                }
            })
            .collect();

        build_symbol_hierarchy(raw_symbols)
    }

    pub fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind {
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

    pub fn find_occurrences(
        &self,
        source: &str,
        tree: &Tree,
        target: &naviscope_core::ingest::parser::SymbolResolution,
    ) -> Vec<naviscope_core::model::Range> {
        let mut ranges = Vec::new();

        // 1. Extract the identifier name and intent
        let (name, intent) = match target {
            naviscope_core::ingest::parser::SymbolResolution::Local(range, _) => {
                // For local symbols, we extract the name directly from the source at the declaration range
                let start = naviscope_core::model::util::line_col_at_to_offset(
                    source,
                    range.start_line,
                    range.start_col,
                );
                let end = naviscope_core::model::util::line_col_at_to_offset(
                    source,
                    range.end_line,
                    range.end_col,
                );

                if let (Some(s), Some(e)) = (start, end) {
                    if s < e && e <= source.len() {
                        (
                            source[s..e].to_string(),
                            naviscope_api::models::SymbolIntent::Variable,
                        )
                    } else {
                        return Vec::new();
                    }
                } else {
                    return Vec::new();
                }
            }
            naviscope_core::ingest::parser::SymbolResolution::Precise(fqn, intent) => {
                (fqn.split('.').last().unwrap_or(fqn).to_string(), *intent)
            }
            naviscope_core::ingest::parser::SymbolResolution::Global(fqn) => {
                // Global resolution from graph usually implies a high-level symbol (Method/Type/Field)
                // We'll try to guess intent if it's not provided, but mostly it will stay broad
                (
                    fqn.split('.').last().unwrap_or(fqn).to_string(),
                    naviscope_api::models::SymbolIntent::Unknown,
                )
            }
        };

        if name.is_empty() {
            return ranges;
        }

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            cursor.matches(&self.occurrence_query, tree.root_node(), source.as_bytes());

        // Mapping from Intent to the capture index we care about
        let target_capture_index = match intent {
            naviscope_api::models::SymbolIntent::Method => Some(self.occurrence_indices.method),
            naviscope_api::models::SymbolIntent::Type => Some(self.occurrence_indices.type_alias),
            naviscope_api::models::SymbolIntent::Field => Some(self.occurrence_indices.field),
            _ => None, // Search all identifiers
        };

        use tree_sitter::StreamingIterator;
        while let Some(mat) = matches.next() {
            // Optimization: If intent is specific, skip matches that don't satisfy the intent structure.
            if let Some(target_idx) = target_capture_index {
                if !mat.captures.iter().any(|c| c.index == target_idx) {
                    continue;
                }
            }

            // Extract the identifier node using our indices
            for cap in mat.captures {
                if cap.index == self.occurrence_indices.ident {
                    if let Ok(text) = cap.node.utf8_text(source.as_bytes()) {
                        if text == name {
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
            }
        }
        ranges
    }
}
