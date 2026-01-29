use crate::parser::SymbolResolution;

/// A generic trait for semantic scopes in any programming language.
/// `C` represents the language-specific resolution context.
pub trait SemanticScope<C>: Send + Sync {
    /// Resolve a name within this specific scope.
    /// Returns:
    /// - `Some(Ok(res))` if the symbol is found.
    /// - `Some(Err(()))` if the symbol is NOT found and searching should stop (shadowing/short-circuit).
    /// - `None` if the symbol is NOT found and searching should continue in the next scope.
    fn resolve(&self, name: &str, context: &C) -> Option<Result<SymbolResolution, ()>>;

    /// Returns the name of the scope for debugging purposes.
    fn name(&self) -> &'static str;
}
