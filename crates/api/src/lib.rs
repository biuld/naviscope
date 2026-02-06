pub mod cache;
pub mod graph;
pub mod lifecycle;
pub mod models;
pub mod navigation;
pub mod semantic;

// Re-export commonly used types
pub use cache::{CacheInspectResult, CacheStats, CachedAssetSummary, StubCacheManager};
pub use graph::GraphService;
pub use lifecycle::EngineLifecycle;
pub use models::*;
pub use navigation::NavigationService;
pub use semantic::{CallHierarchyAnalyzer, ReferenceAnalyzer, SymbolInfoProvider, SymbolNavigator};

/// Composite trait representing the full Naviscope Engine API.
/// This allows clients to depend on a single trait instead of multiple individual ones.
pub trait NaviscopeEngine:
    GraphService
    + NavigationService
    + SymbolNavigator
    + ReferenceAnalyzer
    + CallHierarchyAnalyzer
    + SymbolInfoProvider
    + EngineLifecycle
{
    /// Get the stub cache manager.
    fn get_stub_cache_manager(&self) -> std::sync::Arc<dyn StubCacheManager>;
}
