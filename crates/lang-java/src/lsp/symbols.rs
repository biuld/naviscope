use crate::parser::JavaParser;
use naviscope_api::models::graph::{DisplayGraphNode, NodeKind};
use naviscope_plugin::utils::{RawSymbol, build_symbol_hierarchy, range_from_ts};
use std::collections::HashMap;
use tree_sitter::Tree;

pub fn extract_symbols(parser: &JavaParser, tree: &Tree, source: &str) -> Vec<DisplayGraphNode> {
    // Only run Stage 1: Identification of entities.
    // We don't need full FQN resolution (naming) or relation resolution (Stage 3)
    // for building the local document symbol tree.
    let mut entities = Vec::new();
    let mut relations = Vec::new();
    let mut entities_map = HashMap::new();

    let all_matches = parser.collect_matches(tree, source);

    // Pass None for package to keep FQNs local/relative during symbol extraction
    parser.identify_entities(
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
                range: range_from_ts(e.node.range()),
                selection_range: e
                    .node
                    .child_by_field_name("name")
                    .map(|n| range_from_ts(n.range()))
                    .unwrap_or_else(|| range_from_ts(e.node.range())),
                node: e.node,
            }
        })
        .collect();

    build_symbol_hierarchy(raw_symbols)
}
