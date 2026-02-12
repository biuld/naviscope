//! Local variable type inference.

use super::InferStrategy;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type from local variable declaration.
pub struct LocalVarInfer;

impl InferStrategy for LocalVarInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        // Only works for identifier nodes
        if node.kind() != "identifier" {
            return None;
        }

        let name = node.utf8_text(ctx.source.as_bytes()).ok()?;

        // Optimize: If ScopeManager is available, rely on it exclusively.
        // We do NOT fallback to AST walking if lookup fails, because if a ScopeManager
        // is provided, it is expected to be complete. Fallback would only hide bugs.
        //
        // NOTE: If scope_manager is None, we now return None.
        // This forces integrators to use ScopeManager for local variable inference.
        if let Some(sm) = ctx.scope_manager {
            // Find the nearest scope-owning ancestor
            let mut current = *node;
            while let Some(parent) = current.parent() {
                // Check if this parent node owns a scope
                if sm.get_scope_id(parent.id()).is_some() {
                    // Delegate to ScopeManager to lookup variable starting from this scope
                    // The lookup method will automatically traverse up the scope chain
                    return sm.lookup(parent.id(), name);
                }
                current = parent;
            }
            return None;
        }

        None
    }
}

/// Parse a type node into TypeRef.
pub fn parse_type_node(node: &Node, ctx: &InferContext) -> Option<TypeRef> {
    let kind = node.kind();

    match kind {
        // Primitive types
        "integral_type" | "floating_point_type" | "boolean_type" | "void_type" => {
            let text = node.utf8_text(ctx.source.as_bytes()).ok()?;
            Some(TypeRef::Raw(text.to_string()))
        }
        // Simple type identifier
        "type_identifier" => {
            let name = node.utf8_text(ctx.source.as_bytes()).ok()?;
            // Try to resolve to FQN
            let fqn = ctx
                .ts
                .resolve_type_name(name, &ctx.to_resolution_context())
                .unwrap_or_else(|| name.to_string());
            Some(TypeRef::Id(fqn))
        }
        // Scoped type like java.util.List
        "scoped_type_identifier" => {
            let text = node.utf8_text(ctx.source.as_bytes()).ok()?;
            Some(TypeRef::Id(text.replace(" ", "")))
        }
        // Generic type like List<String>
        "generic_type" => {
            let base_node = node.child_by_field_name("type").or_else(|| node.child(0))?;
            let base = parse_type_node(&base_node, ctx)?;

            let mut args = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "type_arguments" {
                    continue;
                }

                let mut args_cursor = child.walk();
                for arg in child.children(&mut args_cursor) {
                    if !arg.is_named() {
                        continue;
                    }
                    if let Some(parsed) = parse_type_node(&arg, ctx) {
                        args.push(parsed);
                    }
                }
            }

            Some(TypeRef::Generic {
                base: Box::new(base),
                args,
            })
        }
        // Array type
        "array_type" => {
            let element = node.child_by_field_name("element")?;
            let element_type = parse_type_node(&element, ctx)?;
            Some(TypeRef::Array {
                element: Box::new(element_type),
                dimensions: 1, // TODO: count dimensions properly
            })
        }
        _ => {
            // Unknown type node, try raw text
            node.utf8_text(ctx.source.as_bytes())
                .ok()
                .map(|s| TypeRef::Raw(s.to_string()))
        }
    }
}
