//! `new` expression inference.

use super::InferStrategy;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type of `new Type()` expression.
pub struct NewExprInfer;

impl InferStrategy for NewExprInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        if node.kind() != "object_creation_expression" {
            return None;
        }

        // Get the type being constructed
        let type_node = node.child_by_field_name("type")?;
        let type_name = type_node.utf8_text(ctx.source.as_bytes()).ok()?;

        // Handle generic types: get just the base type
        let base_type = if let Some(idx) = type_name.find('<') {
            &type_name[..idx]
        } else {
            type_name
        };

        // Resolve to FQN
        let fqn = ctx
            .ts
            .resolve_type_name(base_type, &ctx.to_resolution_context())
            .unwrap_or_else(|| base_type.to_string());

        Some(TypeRef::Id(fqn))
    }
}
