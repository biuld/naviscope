use crate::inference::adapters::{CodeGraphTypeSystem, NoOpTypeSystem};
use crate::inference::create_inference_context;
use crate::inference::scope::ScopeManager;
use crate::inference::InferContext;
use crate::parser::JavaParser;
use naviscope_api::models::{SymbolIntent, SymbolResolution, TypeRef};
use naviscope_api::models::symbol::Range;
use naviscope_plugin::CodeGraph;
use naviscope_plugin::utils::{line_col_at_to_offset, range_from_ts};
use tree_sitter::{Node, Tree};

pub fn find_occurrences(
    parser: &JavaParser,
    source: &str,
    tree: &Tree,
    target: &SymbolResolution,
    index: Option<&dyn CodeGraph>,
) -> Vec<Range> {
    let mut ranges = Vec::new();

    // 1. Extract package and imports
    let (package, imports) = parser.extract_package_and_imports(tree, source);

    // 2. Build inference context with available type system
    if let Some(index) = index {
        let ts = CodeGraphTypeSystem::new(index);
        let mut scope_manager = ScopeManager::new();
        let ctx = create_inference_context(
            &tree.root_node(),
            source,
            &ts,
            &mut scope_manager,
            package,
            imports,
        );
        collect_occurrences_with_ctx(tree, source, target, &ctx, &mut ranges);
    } else {
        let ts = NoOpTypeSystem;
        let mut scope_manager = ScopeManager::new();
        let ctx = create_inference_context(
            &tree.root_node(),
            source,
            &ts,
            &mut scope_manager,
            package,
            imports,
        );
        collect_occurrences_with_ctx(tree, source, target, &ctx, &mut ranges);
    }

    ranges
}

fn collect_occurrences_with_ctx(
    tree: &Tree,
    source: &str,
    target: &SymbolResolution,
    infer_ctx: &InferContext,
    ranges: &mut Vec<Range>,
) {
    let Some(scope_manager) = infer_ctx.scope_manager else {
        return;
    };

    match target {
        SymbolResolution::Local(decl_range, _decl_name) => {
            if let Some(name) = extract_name_from_range(source, decl_range) {
                find_matching_identifiers(
                    tree,
                    source,
                    &name,
                    |node| {
                        if let Some(scope_id) = find_start_scope_id(node, scope_manager) {
                            if let Some(info) = scope_manager.lookup_symbol(scope_id, &name) {
                                return info.range == *decl_range;
                            }
                        }
                        false
                    },
                    ranges,
                );
            }
        }
        SymbolResolution::Precise(fqn, _) | SymbolResolution::Global(fqn) => {
            let name = fqn
                .split(|c| c == '.' || c == '#' || c == '$')
                .last()
                .unwrap_or(fqn);

            if name.is_empty() {
                return;
            }

            let member_target = fqn.contains('#');
            let type_target = matches!(target, SymbolResolution::Precise(_, SymbolIntent::Type));

            find_matching_identifiers(
                tree,
                source,
                name,
                |node| {
                    if let Some(scope_id) = find_start_scope_id(node, scope_manager) {
                        if scope_manager.lookup_symbol(scope_id, name).is_some() {
                            return false;
                        }
                    }

                    if member_target {
                        return resolve_member_reference_fqn(node, infer_ctx)
                            .map(|resolved| member_fqn_matches_target(&resolved, fqn, infer_ctx))
                            .unwrap_or(false);
                    }

                    if type_target {
                        if let Some(TypeRef::Id(resolved_type)) =
                            crate::inference::strategy::infer_expression(node, infer_ctx)
                        {
                            return resolved_type == *fqn;
                        }
                    }

                    // Name-only fallback for non-member symbols.
                    true
                },
                ranges,
            );
        }
    }
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
    ranges: &mut Vec<Range>,
) where
    F: Fn(&Node) -> bool,
{
    visit_tree_recursive(
        &tree.root_node(),
        source,
        target_name,
        &predicate,
        ranges,
    );
}

fn visit_tree_recursive<F>(
    node: &Node,
    source: &str,
    target_name: &str,
    predicate: &F,
    ranges: &mut Vec<Range>,
) where
    F: Fn(&Node) -> bool,
{
    if node.kind() == "identifier" || node.kind() == "type_identifier" {
        if let Ok(text) = node.utf8_text(source.as_bytes()) {
            if text == target_name {
                if predicate(node) {
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

fn resolve_member_reference_fqn(node: &Node, infer_ctx: &InferContext) -> Option<String> {
    if node.kind() != "identifier" {
        return None;
    }

    if let Some(parent) = node.parent() {
        if parent.kind() == "method_invocation" && parent.child_by_field_name("name") == Some(*node)
        {
            if let Some(resolved) =
                crate::inference::strategy::MethodCallInfer.infer_member(&parent, infer_ctx)
            {
                return Some(resolved);
            }

            // Fallback for implicit `this` calls when enclosing_class is not pre-filled in ctx.
            if parent.child_by_field_name("object").is_none() {
                let member_name = node.utf8_text(infer_ctx.source.as_bytes()).ok()?;
                let class_fqn = find_enclosing_class_fqn(node, infer_ctx)?;
                return Some(crate::naming::build_member_fqn(&class_fqn, member_name));
            }

            return None;
        }

        if parent.kind() == "field_access" && parent.child_by_field_name("field") == Some(*node) {
            return crate::inference::strategy::FieldAccessInfer.infer_member(&parent, infer_ctx);
        }
    }

    resolve_member_declaration_fqn(node, infer_ctx)
}

fn resolve_member_declaration_fqn(node: &Node, infer_ctx: &InferContext) -> Option<String> {
    let parent = node.parent()?;
    if parent.child_by_field_name("name") != Some(*node) {
        return None;
    }

    let member_name = node.utf8_text(infer_ctx.source.as_bytes()).ok()?;
    let class_fqn = find_enclosing_class_fqn(node, infer_ctx)?;

    match parent.kind() {
        "method_declaration" | "constructor_declaration" => {
            Some(crate::naming::build_member_fqn(&class_fqn, member_name))
        }
        "variable_declarator" => {
            if parent.parent().map(|p| p.kind()) == Some("field_declaration") {
                return Some(crate::naming::build_member_fqn(&class_fqn, member_name));
            }
            None
        }
        _ => None,
    }
}

fn find_enclosing_class_fqn(node: &Node, infer_ctx: &InferContext) -> Option<String> {
    let sm = infer_ctx.scope_manager?;
    let start_scope = find_start_scope_id(node, sm)?;
    sm.find_enclosing_class(start_scope)
}

fn member_fqn_matches_target(resolved: &str, target: &str, infer_ctx: &InferContext) -> bool {
    if resolved == target {
        return true;
    }

    let (Some((resolved_owner, resolved_name)), Some((target_owner, target_name))) =
        (split_member_fqn(resolved), split_member_fqn(target))
    else {
        return false;
    };

    if resolved_name != target_name {
        return false;
    }

    let resolved_ty = TypeRef::Id(resolved_owner.to_string());
    let target_ty = TypeRef::Id(target_owner.to_string());

    infer_ctx.ts.is_subtype(&resolved_ty, &target_ty)
        || infer_ctx.ts.is_subtype(&target_ty, &resolved_ty)
}

fn split_member_fqn(fqn: &str) -> Option<(&str, &str)> {
    let (owner, member) = fqn.split_once('#')?;
    Some((owner, member))
}
