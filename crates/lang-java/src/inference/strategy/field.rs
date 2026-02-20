//! Field access inference.

use super::{InferStrategy, infer_expression};
use crate::inference::core::unification::Substitution;
use crate::inference::InferContext;
use crate::inference::TypeRefExt;
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
        let type_fqn = receiver_type.as_fqn()?;

        // Look up the field in the type hierarchy
        let members = ctx.ts.find_member_in_hierarchy(&type_fqn, field_name);
        let mut member = members.first().cloned()?;

        self.apply_receiver_substitution(&mut member, &receiver_type, ctx);
        Some(member)
    }

    fn apply_receiver_substitution(
        &self,
        member: &mut crate::inference::MemberInfo,
        receiver_type: &TypeRef,
        ctx: &InferContext,
    ) {
        let (base_fqn, receiver_args) = match receiver_type {
            TypeRef::Generic { base, args } => {
                let Some(base_fqn) = base.as_fqn() else {
                    return;
                };
                (base_fqn, args)
            }
            _ => return,
        };

        let Some(type_info) = ctx.ts.get_type_info(&base_fqn) else {
            return;
        };

        if type_info.type_parameters.is_empty()
            || type_info.type_parameters.len() != receiver_args.len()
        {
            return;
        }

        let mut subst = Substitution::new();
        for (param, arg) in type_info.type_parameters.iter().zip(receiver_args.iter()) {
            subst.insert(param.name.clone(), arg.clone());
        }

        member.type_ref = subst.apply(&member.type_ref);
    }
}
