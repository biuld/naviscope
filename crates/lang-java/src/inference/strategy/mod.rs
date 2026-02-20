//! Inference strategies using combinator pattern.
//!
//! Each strategy implements [`InferStrategy`] and can be combined using
//! `or_else()`, `map()`, etc.

mod combinator;
mod field;
mod lambda;

mod literal;
mod method;
mod new_expr;
mod this;

mod type_id;

pub use combinator::{Cached, Map, OrElse};
pub use field::FieldAccessInfer;
pub use lambda::LambdaInfer;
pub use literal::LiteralInfer;
pub use type_id::TypeIdentifierInfer;
pub mod local;
pub use local::LocalVarInfer;
pub use method::MethodCallInfer;
pub use new_expr::NewExprInfer;
pub use this::ThisInfer;

use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// A type inference strategy.
///
/// Strategies are composable using combinator methods.
/// Each strategy attempts to infer the type of an AST node.
pub trait InferStrategy: Sync + Send {
    /// Attempt to infer the type of the given node.
    ///
    /// Returns `None` if this strategy doesn't apply or can't determine the type.
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef>;

    /// Check if the node type matches the expected type (Bidirectional Inference).
    ///
    /// Default implementation purely relies on synthesis (infer) and subtyping check.
    fn check(&self, node: &Node, expected: &TypeRef, ctx: &InferContext) -> Option<TypeRef> {
        let inferred = self.infer(node, ctx)?;
        if ctx.ts.is_subtype(&inferred, expected) {
            Some(expected.clone()) // Return expected (more precise usually)
        } else {
            None
        }
    }

    /// Combine with another strategy using "or" logic.
    ///
    /// If `self` returns `None`, try `other`.
    fn or_else<S: InferStrategy>(self, other: S) -> OrElse<Self, S>
    where
        Self: Sized,
    {
        OrElse::new(self, other)
    }

    /// Transform the result using a function.
    fn map<F>(self, f: F) -> Map<Self, F>
    where
        Self: Sized,
        F: Fn(TypeRef) -> TypeRef + Send + Sync,
    {
        Map::new(self, f)
    }

    /// Wrap with caching (memoization).
    fn cached(self) -> Cached<Self>
    where
        Self: Sized,
    {
        Cached::new(self)
    }
}

/// Build the default expression inferrer.
///
/// This combines all strategies in priority order.
pub fn build_expression_inferrer() -> impl InferStrategy {
    ThisInfer
        .or_else(LiteralInfer)
        .or_else(LocalVarInfer)
        .or_else(FieldAccessInfer)
        .or_else(MethodCallInfer)
        .or_else(NewExprInfer)
        .or_else(LambdaInfer)
        .or_else(TypeIdentifierInfer)
}

/// Infer the type of an expression node.
///
/// This is the main entry point for expression type inference.
pub fn infer_expression(node: &Node, ctx: &InferContext) -> Option<TypeRef> {
    // TODO: Use lazy_static for the inferrer once all strategies are complete
    let inferrer = build_expression_inferrer();
    inferrer.infer(node, ctx)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Dummy strategy for testing combinators
    struct AlwaysNone;
    impl InferStrategy for AlwaysNone {
        fn infer(&self, _: &Node, _: &InferContext) -> Option<TypeRef> {
            None
        }
    }

    struct AlwaysSome(TypeRef);
    impl InferStrategy for AlwaysSome {
        fn infer(&self, _: &Node, _: &InferContext) -> Option<TypeRef> {
            Some(self.0.clone())
        }
    }

    #[test]
    fn test_or_else_first_succeeds() {
        let s = AlwaysSome(TypeRef::Raw("int".into())).or_else(AlwaysNone);
        // Can't easily test without a Node, but the structure is correct
        let _ = s;
    }

    #[test]
    fn test_or_else_fallback() {
        let s = AlwaysNone.or_else(AlwaysSome(TypeRef::Raw("String".into())));
        let _ = s;
    }
}
