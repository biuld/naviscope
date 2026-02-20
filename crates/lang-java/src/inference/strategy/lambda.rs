//! Lambda expression type inference.
//!
//! Requires checking mode (bidirectional inference) as lambdas don't have intrinsic types.

use crate::inference::{InferContext, InferStrategy, TypeKind, TypeRefExt};
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Strategy to infer types of lambda expressions.
pub struct LambdaInfer;

impl InferStrategy for LambdaInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        // Lambdas do not possess a standalone type; they require a target type.
        // If we are in synthesis mode (infer), we check if the context provides an expected type.
        if let Some(expected) = &ctx.expected_type {
            return self.check(node, expected, ctx);
        }
        None
    }

    fn check(&self, node: &Node, expected: &TypeRef, ctx: &InferContext) -> Option<TypeRef> {
        if node.kind() != "lambda_expression" {
            return None;
        }

        let expected_fqn = expected.as_fqn()?;

        let type_info = ctx.ts.get_type_info(&expected_fqn)?;

        // Basic check: is it an interface?
        // This is a heuristic. A proper check would verify functional interface status.
        if type_info.kind == TypeKind::Interface {
            // Return the expected type as the inferred type of the lambda
            Some(expected.clone())
        } else {
            None
        }
    }
}
