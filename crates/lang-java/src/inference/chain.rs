//! Chain resolution for method chaining.
//!
//! Uses functional unfold pattern to resolve chains like:
//! `response.getContext().get("key")`

use crate::inference::strategy::infer_expression;
use crate::inference::{InferContext, TypeRefExt};
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Result of resolving a chain.
#[derive(Debug, Clone)]
pub struct ChainResolution {
    /// The final member FQN (e.g., "SessionContext#get")  
    pub member_fqn: Option<String>,
    /// The result type of the chain
    pub result_type: TypeRef,
}

impl ChainResolution {
    /// Create from just a type (for simple expressions)
    pub fn from_type(ty: TypeRef) -> Self {
        Self {
            member_fqn: None,
            result_type: ty,
        }
    }

    /// Create with both member and type
    pub fn with_member(member_fqn: String, result_type: TypeRef) -> Self {
        Self {
            member_fqn: Some(member_fqn),
            result_type,
        }
    }
}

/// A step in the chain resolution process.
#[allow(dead_code)] // WithReceiver variant is part of incomplete chain resolution
enum ChainStep<'a> {
    /// Initial node to resolve
    Initial(&'a Node<'a>),
    /// Have a receiver type, resolving member
    WithReceiver {
        receiver_type: TypeRef,
        node: &'a Node<'a>,
    },
    /// Final resolution
    Resolved(ChainResolution),
}

impl<'a> ChainStep<'a> {
    /// Get the resolution if this is the final state.
    fn resolution(&self) -> Option<ChainResolution> {
        match self {
            ChainStep::Resolved(r) => Some(r.clone()),
            _ => None,
        }
    }

    /// Advance to the next step.
    fn next(&self, ctx: &InferContext) -> Option<ChainStep<'a>> {
        match self {
            ChainStep::Initial(node) => {
                // Infer type of initial expression
                let ty = infer_expression(node, ctx)?;

                // Check if there's a parent chain node
                if let Some(parent) = node.parent() {
                    if is_chain_parent(parent.kind()) {
                        // Store reference to parent - this is tricky with lifetimes
                        // For now, resolve immediately
                        return Some(ChainStep::Resolved(ChainResolution::from_type(ty)));
                    }
                }

                Some(ChainStep::Resolved(ChainResolution::from_type(ty)))
            }

            ChainStep::WithReceiver {
                receiver_type,
                node,
            } => {
                // Get member name from the node
                let member_name = extract_member_name(node, ctx)?;
                let type_fqn = receiver_type.as_fqn()?;

                // Find member in hierarchy
                let members = ctx.ts.find_member_in_hierarchy(&type_fqn, &member_name);
                let member = members.first()?;

                // Check for more chain
                if let Some(parent) = node.parent() {
                    if is_chain_parent(parent.kind()) {
                        // Continue chain - simplified for now
                        return Some(ChainStep::Resolved(ChainResolution::with_member(
                            member.fqn.clone(),
                            member.type_ref.clone(),
                        )));
                    }
                }

                Some(ChainStep::Resolved(ChainResolution::with_member(
                    member.fqn.clone(),
                    member.type_ref.clone(),
                )))
            }

            ChainStep::Resolved(_) => None, // Terminal state
        }
    }
}

/// Check if a node kind is a chain parent (method_invocation, field_access).
fn is_chain_parent(kind: &str) -> bool {
    matches!(kind, "method_invocation" | "field_access")
}

/// Extract member name from a chain node.
fn extract_member_name(node: &Node, ctx: &InferContext) -> Option<String> {
    let name_node = match node.kind() {
        "method_invocation" => node.child_by_field_name("name"),
        "field_access" => node.child_by_field_name("field"),
        _ => None,
    }?;

    name_node
        .utf8_text(ctx.source.as_bytes())
        .ok()
        .map(|s| s.to_string())
}

/// Resolve a chain of method calls / field accesses.
///
/// Uses `std::iter::successors` for functional unfold.
///
/// # Example
///
/// For `response.getContext().get("key")`:
/// 1. Resolve `response` → HttpResponseMessage
/// 2. Find `getContext` in HttpResponseMessage hierarchy → SessionContext
/// 3. Find `get` in SessionContext hierarchy → Object
pub fn resolve_chain<'a>(
    initial: &'a Node<'a>,
    ctx: &'a InferContext<'a>,
) -> Option<ChainResolution> {
    const MAX_DEPTH: usize = 20;

    // Use successors to unfold the chain
    let steps: Vec<_> =
        std::iter::successors(Some(ChainStep::Initial(initial)), |step| step.next(ctx))
            .take(MAX_DEPTH)
            .collect();

    // Return the last resolved step
    steps.into_iter().rev().find_map(|s| s.resolution())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chain_resolution_from_type() {
        let res = ChainResolution::from_type(TypeRef::Id("String".into()));
        assert!(res.member_fqn.is_none());
    }

    #[test]
    fn test_chain_resolution_with_member() {
        let res = ChainResolution::with_member("List#get".into(), TypeRef::Id("Object".into()));
        assert_eq!(res.member_fqn, Some("List#get".into()));
    }
}
