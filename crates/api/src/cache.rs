use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Summary of a cached asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedAssetSummary {
    pub hash: String,
    pub path: String,
    pub size_bytes: u64,
    pub stub_count: usize,
    pub version: u32,
    pub created_at: u64,
}

/// Detailed inspection result for a cached asset
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheInspectResult {
    pub summary: CachedAssetSummary,
    pub metadata_distribution: HashMap<String, usize>,
    pub sample_entries: Vec<String>,
}

/// Statistics for the global stub cache
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStats {
    pub total_assets: usize,
    pub total_entries: usize,
    pub cache_dir: PathBuf,
}

/// Service interface for managing the global stub cache
pub trait StubCacheManager: Send + Sync {
    /// Get cache statistics
    fn stats(&self) -> CacheStats;

    /// Scan all cached assets returning their summaries
    fn scan_assets(&self) -> Vec<CachedAssetSummary>;

    /// Inspect a specific cached asset by hash (full or prefix)
    fn inspect_asset(&self, hash_prefix: &str) -> Option<CacheInspectResult>;

    /// Clear all cached data
    fn clear(&self) -> Result<(), String>;
}
