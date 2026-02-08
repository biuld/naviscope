use crate::inference::adapters::NoOpTypeSystem;
use crate::inference::create_inference_context;
use crate::inference::scope::ScopeManager;
use crate::parser::JavaParser;
use naviscope_api::models::SymbolResolution;
use naviscope_api::models::symbol::Range;
use naviscope_plugin::utils::{line_col_at_to_offset, range_from_ts};
use tree_sitter::{Node, Tree};

pub fn find_occurrences(
    parser: &JavaParser,
    source: &str,
    tree: &Tree,
    target: &SymbolResolution,
) -> Vec<Range> {
    let mut ranges = Vec::new();

    // 1. Setup Type System & Scope Manager
    // We use NoOpTypeSystem because LspParser typically doesn't have access to the global index.
    // This is sufficient for precise local variable resolution.
    let ts = NoOpTypeSystem;
    let mut scope_manager = ScopeManager::new();

    // 2. Extract package and imports
    let (package, imports) = parser.extract_package_and_imports(tree, source);

    // 3. Build Inference Context (populates ScopeManager with all local definitions)
    let _ctx = create_inference_context(
        &tree.root_node(),
        source,
        &ts,
        &mut scope_manager,
        package,
        imports,
    );

    // 4. Handle based on resolution type
    match target {
        SymbolResolution::Local(decl_range, _decl_name) => {
            // Case A: Local Variable
            // We want to find all usages that resolve to THIS specific declaration range.

            // Extract the variable name from the source if possible
            if let Some(name) = extract_name_from_range(source, decl_range) {
                // Find all identifiers that match the name
                find_matching_identifiers(
                    tree,
                    source,
                    &name,
                    |node, sm| {
                        // Start scope search from this node's scope
                        if let Some(scope_id) = find_start_scope_id(node, sm) {
                            if let Some(info) = sm.lookup_symbol(scope_id, &name) {
                                // MATCH condition: The resolved symbol must have the exact same declaration range
                                if info.range == *decl_range {
                                    return true;
                                }
                            }
                        }
                        false
                    },
                    &scope_manager,
                    &mut ranges,
                );

                // Also add the declaration itself if not already covered (usually identifiers cover it)
                // But let's ensure we don't duplicate. find_matching_identifiers scans all nodes.
            }
        }
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => {
            // Case B: Global/Member Symbol
            // We want to find usages that DO NOT resolve to a local variable.
            // Since we don't have full type resolution, this is a "best effort" semantic search.

            let name = fqn
                .split(|c| c == '.' || c == '#' || c == '$')
                .last()
                .unwrap_or(fqn);

            if name.is_empty() {
                return ranges;
            }

            find_matching_identifiers(
                tree,
                source,
                name,
                |node, sm| {
                    if let Some(scope_id) = find_start_scope_id(node, sm) {
                        // If it resolves to a LOCAL variable, it is SHADOWED -> Not a match
                        if sm.lookup_symbol(scope_id, name).is_some() {
                            return false;
                        }
                    }
                    // If not locally resolved, it refers to a field/method/class.
                    // Without global index, we assume it matches the target FQN if name matches.
                    // Improvement: Check imports?
                    true
                },
                &scope_manager,
                &mut ranges,
            );
        }
    }

    ranges
}

fn extract_name_from_range(source: &str, range: &Range) -> Option<String> {
    let start = line_col_at_to_offset(source, range.start_line, range.start_col)?;
    let end = line_col_at_to_offset(source, range.end_line, range.end_col)?;
    if start < end && end <= source.len() {
        Some(source[start..end].to_string())
    } else {
        None
    }
}

fn find_matching_identifiers<F>(
    tree: &Tree,
    source: &str,
    target_name: &str,
    predicate: F,
    scope_manager: &ScopeManager,
    ranges: &mut Vec<Range>,
) where
    F: Fn(&Node, &ScopeManager) -> bool,
{
    // A simple recursive walker is sufficient
    visit_tree_recursive(
        &tree.root_node(),
        source,
        target_name,
        &predicate,
        scope_manager,
        ranges,
    );
}

fn visit_tree_recursive<F>(
    node: &Node,
    source: &str,
    target_name: &str,
    predicate: &F,
    scope_manager: &ScopeManager,
    ranges: &mut Vec<Range>,
) where
    F: Fn(&Node, &ScopeManager) -> bool,
{
    if node.kind() == "identifier" || node.kind() == "type_identifier" {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            if text == target_name {
                if predicate(node, scope_manager) {
                    ranges.push(range_from_ts(node.range()));
                }
            }
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        visit_tree_recursive(
            &child,
            source,
            target_name,
            predicate,
            scope_manager,
            ranges,
        );
    }
}

fn find_start_scope_id(node: &Node, sm: &ScopeManager) -> Option<usize> {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if let Some(sid) = sm.get_scope_id(parent.id()) {
            return Some(sid);
        }
        current = parent;
    }
    None
}
