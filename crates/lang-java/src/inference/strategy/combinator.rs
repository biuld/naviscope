//! Combinator implementations for InferStrategy.

use super::InferStrategy;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Or-else combinator: try first, then second if first returns None.
pub struct OrElse<A, B> {
    first: A,
    second: B,
}

impl<A, B> OrElse<A, B> {
    pub fn new(first: A, second: B) -> Self {
        Self { first, second }
    }
}

impl<A: InferStrategy, B: InferStrategy> InferStrategy for OrElse<A, B> {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        self.first
            .infer(node, ctx)
            .or_else(|| self.second.infer(node, ctx))
    }
}

// Implement Send + Sync if components are
unsafe impl<A: Send, B: Send> Send for OrElse<A, B> {}
unsafe impl<A: Sync, B: Sync> Sync for OrElse<A, B> {}

/// Map combinator: transform the result.
pub struct Map<S, F> {
    strategy: S,
    f: F,
}

impl<S, F> Map<S, F> {
    pub fn new(strategy: S, f: F) -> Self {
        Self { strategy, f }
    }
}

impl<S: InferStrategy, F: Fn(TypeRef) -> TypeRef + Send + Sync> InferStrategy for Map<S, F> {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        self.strategy.infer(node, ctx).map(&self.f)
    }
}

/// Cached combinator: memoize results.
///
/// Note: This is a placeholder. Full caching would need thread-safe storage.
pub struct Cached<S> {
    strategy: S,
    // TODO: Add actual cache with HashMap<NodeId, Option<TypeRef>>
}

impl<S> Cached<S> {
    pub fn new(strategy: S) -> Self {
        Self { strategy }
    }
}

impl<S: InferStrategy> InferStrategy for Cached<S> {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        // TODO: Check cache first, then delegate
        self.strategy.infer(node, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ReturnInt;
    impl InferStrategy for ReturnInt {
        fn infer(&self, _: &Node, _: &InferContext) -> Option<TypeRef> {
            Some(TypeRef::Raw("int".into()))
        }
    }

    #[test]
    fn test_map_transforms_result() {
        let mapped = Map::new(ReturnInt, |_: TypeRef| TypeRef::Raw("long".into()));
        // Structure test - actual inference needs a Node
        let _ = mapped;
    }
}
