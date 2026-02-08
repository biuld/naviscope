//! `this` expression inference.

use super::InferStrategy;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type of `this` expression.
pub struct ThisInfer;

impl InferStrategy for ThisInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        if node.kind() != "this" {
            return None;
        }

        // Return the enclosing class type
        ctx.enclosing_class.as_ref().map(|c| TypeRef::Id(c.clone()))
    }
}
