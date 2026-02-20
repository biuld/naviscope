use crate::error::ApiResult;
use async_trait::async_trait;

/// Result of resolving a user-provided path to a node FQN.
/// This is specific to CLI-style navigation with fuzzy matching and relative paths.
#[derive(Debug, Clone)]
pub enum ResolveResult {
    /// Exactly one node found
    Found(String),
    /// Multiple nodes match the query (ambiguous)
    Ambiguous(Vec<String>),
    /// No node found
    NotFound,
}

/// Navigation service for CLI-style path resolution.
///
/// This trait provides fuzzy matching and relative path resolution for interactive shells.
/// For structured graph queries (get children, get parent, etc.), use `GraphService` instead.
///
/// # Design Rationale
/// - `resolve_path()` supports CLI-specific features like:
///   - Fuzzy matching: "MyClass" finds "com.example.MyClass"
///   - Relative paths: "../sibling" from current context
///   - Special paths: "/", "root"
/// - These features don't fit well into structured `GraphQuery` patterns
/// - Other graph traversal operations should use `GraphService::query()`
#[async_trait]
pub trait NavigationService: Send + Sync {
    /// Resolve a user-provided path to a concrete FQN.
    ///
    /// Supports multiple resolution strategies:
    /// - **Absolute FQN**: "com.example.MyClass" → exact match
    /// - **Relative path**: "../sibling" from current context
    /// - **Fuzzy name**: "MyClass" → searches for matching nodes
    /// - **Special paths**: "/" or "root" → project root
    ///
    /// # Arguments
    /// * `target` - The path to resolve (e.g., "MyClass", "../sibling", "/root/package")
    /// * `current_context` - Optional current FQN for relative path resolution
    ///
    /// # Returns
    /// * `ResolveResult::Found(fqn)` - Exactly one match found
    /// * `ResolveResult::Ambiguous(fqns)` - Multiple matches found (user needs to disambiguate)
    /// * `ResolveResult::NotFound` - No matches found
    ///
    /// # Example
    /// ```ignore
    /// // Fuzzy match
    /// service.resolve_path("MyClass", None).await
    /// // => Found("com.example.MyClass")
    ///
    /// // Relative path
    /// service.resolve_path("../OtherClass", Some("com.example.MyClass")).await
    /// // => Found("com.example.OtherClass")
    /// ```
    async fn resolve_path(
        &self,
        target: &str,
        current_context: Option<&str>,
    ) -> ApiResult<ResolveResult>;

    /// Get completion candidates for a prefix.
    async fn get_completion_candidates(&self, prefix: &str, limit: usize)
    -> ApiResult<Vec<String>>;
}
