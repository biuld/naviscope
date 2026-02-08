//! Field access inference.

use super::{InferStrategy, infer_expression};
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type from field access expression (obj.field).
pub struct FieldAccessInfer;

impl InferStrategy for FieldAccessInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        let member = self.resolve_member(node, ctx)?;
        Some(member.type_ref.clone())
    }
}

impl FieldAccessInfer {
    /// Resolve the field access to its member definition.
    pub fn infer_member(&self, node: &Node, ctx: &InferContext) -> Option<String> {
        let member = self.resolve_member(node, ctx)?;
        Some(member.fqn.clone())
    }

    fn resolve_member(
        &self,
        node: &Node,
        ctx: &InferContext,
    ) -> Option<crate::inference::MemberInfo> {
        let field_node = if node.kind() == "field_access" {
            *node
        } else if let Some(parent) = node.parent() {
            if parent.kind() == "field_access" && parent.child_by_field_name("field") == Some(*node)
            {
                parent
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Get the receiver (object before the dot)
        let receiver = field_node.child_by_field_name("object")?;

        // Get the field name
        let name_node = field_node.child_by_field_name("field")?;
        let field_name = name_node.utf8_text(ctx.source.as_bytes()).ok()?;

        // Infer the receiver type (recursive)
        let receiver_type = infer_expression(&receiver, ctx)?;

        // Get the FQN from the receiver type
        let type_fqn = match &receiver_type {
            TypeRef::Id(fqn) => fqn.clone(),
            _ => return None,
        };

        // Look up the field in the type hierarchy
        let members = ctx.ts.find_member_in_hierarchy(&type_fqn, field_name);
        members.first().cloned()
    }
}
