use crate::error::{NaviscopeError, Result};
use crate::model::{NodeKind, Range};
use tree_sitter::{Language, Query};

/// Converts a tree-sitter range to our internal Range model.
pub fn range_from_ts(range: tree_sitter::Range) -> Range {
    Range {
        start_line: range.start_point.row,
        start_col: range.start_point.column,
        end_line: range.end_point.row,
        end_col: range.end_point.column,
    }
}

/// Loads a Tree-sitter query from an SCM string.
pub fn load_query(language: &Language, scm: &str) -> Result<Query> {
    Query::new(language, scm)
        .map_err(|e| NaviscopeError::Parsing(format!("Invalid query: {:?}", e)))
}

/// Gets the index of a capture name in a query.
pub fn get_capture_index(query: &Query, name: &str) -> Result<u32> {
    query
        .capture_index_for_name(name)
        .ok_or_else(|| NaviscopeError::Parsing(format!("Capture name '{}' not found in SCM", name)))
}

/// A raw symbol representation used during tree construction.
pub struct RawSymbol<'a> {
    pub name: String,
    pub kind: NodeKind,
    pub range: crate::model::Range,
    pub selection_range: crate::model::Range,
    pub node: tree_sitter::Node<'a>,
}

/// Builds a hierarchical DisplayGraphNode tree from flat raw symbols using AST parent-child relationships.
pub fn build_symbol_hierarchy(raw_symbols: Vec<RawSymbol>) -> Vec<crate::model::DisplayGraphNode> {
    use std::collections::HashMap;
    let mut symbols_map: HashMap<usize, usize> = HashMap::new(); // node_id -> flat_index
    let mut flat_symbols: Vec<crate::model::DisplayGraphNode> = Vec::new();
    let mut parent_child_rels: Vec<(usize, usize)> = Vec::new();

    // 1. Create flat list and map nodes to indices
    for (i, raw) in raw_symbols.iter().enumerate() {
        flat_symbols.push(crate::model::DisplayGraphNode {
            id: raw.name.clone(), // For document symbols, FQN might not be available, use name as fallback id
            name: raw.name.clone(),
            kind: raw.kind.clone(),
            lang: String::new(), // To be filled by caller if needed
            location: Some(crate::model::DisplaySymbolLocation {
                path: String::new(), // To be filled by caller
                range: raw.range,
                selection_range: Some(raw.selection_range),
            }),
            metadata: serde_json::Value::Null,
            detail: None,
            signature: None,
            modifiers: vec![],
            children: Some(Vec::new()),
        });
        symbols_map.insert(raw.node.id(), i);
    }

    // 2. Determine parent-child relationships using AST
    for (i, raw) in raw_symbols.iter().enumerate() {
        let mut curr = raw.node;
        while let Some(parent) = curr.parent() {
            if let Some(&parent_idx) = symbols_map.get(&parent.id()) {
                if parent_idx != i {
                    parent_child_rels.push((parent_idx, i));
                    break;
                }
            }
            curr = parent;
        }
    }

    // 3. Build the tree
    let mut has_parent = vec![false; flat_symbols.len()];
    for (_p, c) in &parent_child_rels {
        has_parent[*c] = true;
    }

    let mut roots = Vec::new();
    for i in 0..flat_symbols.len() {
        if !has_parent[i] {
            roots.push(i);
        }
    }

    fn build_node(
        idx: usize,
        flat: &mut Vec<crate::model::DisplayGraphNode>,
        rels: &[(usize, usize)],
    ) -> crate::model::DisplayGraphNode {
        let mut sym = flat[idx].clone();
        let children: Vec<usize> = rels
            .iter()
            .filter(|(p, _)| *p == idx)
            .map(|(_, c)| *c)
            .collect();
        let mut child_nodes = Vec::new();
        for c_idx in children {
            child_nodes.push(build_node(c_idx, flat, rels));
        }
        sym.children = if child_nodes.is_empty() {
            None
        } else {
            Some(child_nodes)
        };
        sym
    }

    roots
        .into_iter()
        .map(|root_idx| build_node(root_idx, &mut flat_symbols, &parent_child_rels))
        .collect()
}

/// Macro to define a struct for capture indices and a `new` method to initialize it from a query.
#[macro_export]
macro_rules! decl_indices {
    ($name:ident, { $($field:ident => $capture:expr),+ $(,)? }) => {
        #[derive(Clone)]
        pub struct $name {
            $(pub $field: u32,)+
        }

        impl $name {
            pub fn new(query: &tree_sitter::Query) -> $crate::error::Result<Self> {
                Ok(Self {
                    $($field: $crate::parser::utils::get_capture_index(query, $capture)?,)+
                })
            }
        }
    };
}
