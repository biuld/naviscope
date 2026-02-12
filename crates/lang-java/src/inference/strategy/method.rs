//! Method invocation inference.

use super::{InferStrategy, infer_expression};
use crate::inference::core::unification::Substitution;
use crate::inference::InferContext;
use crate::inference::TypeRefExt;
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
        let type_fqn = receiver_type.as_fqn()?;

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
        if candidates.is_empty() {
            return None;
        }

        let candidates = self.apply_receiver_substitution(candidates, &receiver_type, ctx);

        // Resolve the best match among candidates
        ctx.ts.resolve_method(&candidates, &arg_types)
    }

    fn apply_receiver_substitution(
        &self,
        candidates: Vec<crate::inference::MemberInfo>,
        receiver_type: &TypeRef,
        ctx: &InferContext,
    ) -> Vec<crate::inference::MemberInfo> {
        let (base_fqn, receiver_args) = match receiver_type {
            TypeRef::Generic { base, args } => {
                let Some(base_fqn) = base.as_fqn() else {
                    return candidates;
                };
                (base_fqn, args)
            }
            _ => return candidates,
        };

        let Some(type_info) = ctx.ts.get_type_info(&base_fqn) else {
            return candidates;
        };

        if type_info.type_parameters.is_empty()
            || type_info.type_parameters.len() != receiver_args.len()
        {
            return candidates;
        }

        let mut subst = Substitution::new();
        for (param, arg) in type_info.type_parameters.iter().zip(receiver_args.iter()) {
            subst.insert(param.name.clone(), arg.clone());
        }

        candidates
            .into_iter()
            .map(|mut member| {
                member.type_ref = subst.apply(&member.type_ref);
                if let Some(params) = &mut member.parameters {
                    for p in params {
                        p.type_ref = subst.apply(&p.type_ref);
                    }
                }
                member
            })
            .collect()
    }
}
