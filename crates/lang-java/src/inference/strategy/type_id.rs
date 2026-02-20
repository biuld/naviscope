use crate::inference::InferContext;
use crate::inference::strategy::InferStrategy;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infers the type of a plain identifier by looking it up in the type system.
///
/// This is used for type names in various contexts (declarations, casts, etc.)
/// and as a final fallback for expressions that might be static type references.
pub struct TypeIdentifierInfer;

impl InferStrategy for TypeIdentifierInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        if !matches!(node.kind(), "identifier" | "type_identifier") {
            return None;
        }

        let name = node.utf8_text(ctx.source.as_bytes()).ok()?;
        let res_ctx = ctx.to_resolution_context();

        // 0. Check if this is part of a scoped type (e.g., Outer.Inner)
        if let Some(parent) = node.parent() {
            if parent.kind() == "scoped_type_identifier" {
                // Get the full scoped name
                let full_name = parent.utf8_text(ctx.source.as_bytes()).ok()?;
                // Replace '.' with '$' for inner class naming convention if needed
                let normalized = full_name.replace(" ", "");

                // Try to resolve the full path
                if let Some(fqn) = ctx.ts.resolve_type_name(&normalized, &res_ctx) {
                    return Some(TypeRef::Id(fqn));
                }

                // Try with inner class convention (Outer$Inner or Outer.Inner)
                if let Some(fqn) = ctx
                    .ts
                    .resolve_type_name(&normalized.replace(".", "$"), &res_ctx)
                {
                    return Some(TypeRef::Id(fqn));
                }
            }
        }

        // 1. Try to resolve as a type name
        if let Some(fqn) = ctx.ts.resolve_type_name(name, &res_ctx) {
            return Some(TypeRef::Id(fqn));
        }

        // 2. Try to resolve as an implicit this.field
        if let Some(enclosing) = &ctx.enclosing_class {
            let members = ctx.ts.find_member_in_hierarchy(enclosing, name);
            if let Some(member) = members.first() {
                return Some(member.type_ref.clone());
            }
        }

        None
    }
}
