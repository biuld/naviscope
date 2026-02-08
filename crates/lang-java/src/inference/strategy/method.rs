//! Method invocation inference.

use super::{InferStrategy, infer_expression};
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type from method invocation (obj.method() or method()).
pub struct MethodCallInfer;

impl InferStrategy for MethodCallInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        let member = self.resolve_member(node, ctx)?;
        Some(member.type_ref)
    }
}

impl MethodCallInfer {
    /// Resolve the method call to its member definition.
    pub fn infer_member(&self, node: &Node, ctx: &InferContext) -> Option<String> {
        let member = self.resolve_member(node, ctx)?;
        Some(member.fqn)
    }

    fn resolve_member(
        &self,
        node: &Node,
        ctx: &InferContext,
    ) -> Option<crate::inference::MemberInfo> {
        let call_node = if node.kind() == "method_invocation" {
            *node
        } else if let Some(parent) = node.parent() {
            if parent.kind() == "method_invocation"
                && parent.child_by_field_name("name") == Some(*node)
            {
                parent
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Get method name
        let name_node = call_node.child_by_field_name("name")?;
        let method_name = name_node.utf8_text(ctx.source.as_bytes()).ok()?;

        // Get receiver, if any
        let receiver_type = if let Some(receiver) = call_node.child_by_field_name("object") {
            // obj.method()
            infer_expression(&receiver, ctx)?
        } else {
            // method() - implicit this or static import
            if let Some(ref enclosing) = ctx.enclosing_class {
                TypeRef::Id(enclosing.clone())
            } else {
                return None;
            }
        };

        // Get the FQN from the receiver type
        let type_fqn = match &receiver_type {
            TypeRef::Id(fqn) => fqn.clone(),
            _ => return None,
        };

        // Get argument types
        let mut arg_types = Vec::new();
        if let Some(args_node) = call_node.child_by_field_name("arguments") {
            let mut cursor = args_node.walk();
            for child in args_node.children(&mut cursor) {
                if child.is_named() {
                    if let Some(t) = infer_expression(&child, ctx) {
                        arg_types.push(t);
                    } else {
                        arg_types.push(TypeRef::Unknown);
                    }
                }
            }
        }

        // Look up the method candidates in the type hierarchy
        let candidates = ctx.ts.find_member_in_hierarchy(&type_fqn, method_name);

        // Resolve the best match among candidates
        ctx.ts.resolve_method(&candidates, &arg_types)
    }
}
