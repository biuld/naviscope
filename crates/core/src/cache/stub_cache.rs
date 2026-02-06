//! Global stub cache for external dependencies
//!
//! Stores parsed stub data from external assets (JARs, jmods, etc.) in a global cache
//! to avoid re-parsing the same dependencies across different projects.

use naviscope_plugin::IndexNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;
use xxhash_rust::xxh3::xxh3_64;

// Note: We use the metadata registry from naviscope_plugin

/// Key identifying an external asset
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub struct AssetKey {
    pub path: PathBuf,
    pub size: u64,
    pub mtime: u64, // Unix timestamp for serialization simplicity
}

impl AssetKey {
    /// Create an AssetKey from a file path
    pub fn from_path(path: &Path) -> std::io::Result<Self> {
        let metadata = fs::metadata(path)?;
        let mtime = metadata
            .modified()?
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(Self {
            path: path.to_path_buf(),
            size: metadata.len(),
            mtime,
        })
    }

    /// Compute a hash for this asset key
    pub fn hash(&self) -> u64 {
        let key_str = format!("{}:{}:{}", self.path.display(), self.size, self.mtime);
        xxh3_64(key_str.as_bytes())
    }
}

/// Cached stub entry (serializable, language-agnostic)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedStub {
    pub fqn: String,
    pub id: String,
    pub name: String,
    pub kind: naviscope_api::models::graph::NodeKind,
    pub lang: naviscope_api::models::Language,
    pub source: naviscope_api::models::graph::NodeSource,
    pub status: naviscope_api::models::graph::ResolutionStatus,
    /// Encapsulated metadata
    pub metadata: naviscope_plugin::CachedMetadata,
}

impl CachedStub {
    /// Convert from IndexNode (language-agnostic serialization)
    pub fn from_index_node(node: &IndexNode) -> Self {
        let id = match &node.id {
            naviscope_api::models::symbol::NodeId::Flat(s) => s.clone(),
            naviscope_api::models::symbol::NodeId::Structured(s) => format!("{:?}", s),
        };

        // Use trait method for serialization
        let metadata = node.metadata.to_cached_metadata();

        Self {
            fqn: id.clone(),
            id,
            name: node.name.clone(),
            kind: node.kind.clone(),
            lang: naviscope_api::models::Language::from(node.lang.clone()),
            source: node.source.clone(),
            status: node.status.clone(),
            metadata,
        }
    }

    /// Convert back to IndexNode
    pub fn to_index_node(&self) -> IndexNode {
        // Deserialize metadata
        let metadata = naviscope_plugin::deserialize_metadata(&self.metadata);

        IndexNode {
            id: naviscope_api::models::symbol::NodeId::Flat(self.id.clone()),
            name: self.name.clone(),
            kind: self.kind.clone(),
            lang: self.lang.to_string(),
            source: self.source.clone(),
            status: self.status.clone(),
            location: None,
            metadata,
        }
    }
}

/// Cache file for a single asset
#[derive(Debug, Serialize, Deserialize)]
pub struct StubCacheFile {
    pub version: u32,
    pub asset_hash: u64,
    pub asset_path: String,
    pub created_at: u64,
    pub entries: HashMap<String, CachedStub>,
}

impl StubCacheFile {
    pub fn new(asset: &AssetKey) -> Self {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Self {
            version: 1,
            asset_hash: asset.hash(),
            asset_path: asset.path.display().to_string(),
            created_at: now,
            entries: HashMap::new(),
        }
    }
}

/// Global stub cache manager
pub struct GlobalStubCache {
    base_dir: PathBuf,
    loaded: Arc<RwLock<HashMap<u64, Arc<RwLock<StubCacheFile>>>>>,
}

use naviscope_api::cache::{CacheInspectResult, CacheStats, CachedAssetSummary, StubCacheManager};

impl StubCacheManager for GlobalStubCache {
    fn stats(&self) -> CacheStats {
        self.stats()
    }

    fn scan_assets(&self) -> Vec<CachedAssetSummary> {
        self.scan_assets()
    }

    fn inspect_asset(&self, hash_prefix: &str) -> Option<CacheInspectResult> {
        self.inspect_asset(hash_prefix)
    }

    fn clear(&self) -> Result<(), String> {
        self.clear().map_err(|e| e.to_string())
    }
}

impl GlobalStubCache {
    /// Create a new global stub cache
    pub fn new(base_dir: PathBuf) -> Self {
        fs::create_dir_all(&base_dir).unwrap_or_default();
        Self {
            base_dir,
            loaded: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Get the default global cache location
    pub fn default_location() -> PathBuf {
        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".naviscope").join("stub_cache")
    }

    /// Create a global cache at the default location
    pub fn at_default_location() -> Self {
        Self::new(Self::default_location())
    }

    /// Get the cache file path for an asset
    fn cache_path(&self, asset_hash: u64) -> PathBuf {
        self.base_dir.join(format!("{:016x}.stubs", asset_hash))
    }

    /// Load or create cache for an asset
    fn get_or_create_cache(&self, asset: &AssetKey) -> Arc<RwLock<StubCacheFile>> {
        let hash = asset.hash();

        // Check if already loaded
        {
            let loaded = self.loaded.read().unwrap();
            if let Some(cache) = loaded.get(&hash) {
                return cache.clone();
            }
        }

        // Try to load from disk
        let cache_path = self.cache_path(hash);
        let cache = if cache_path.exists() {
            match fs::read(&cache_path) {
                Ok(bytes) => match rmp_serde::from_slice::<StubCacheFile>(&bytes) {
                    Ok(file) if file.asset_hash == hash => file,
                    _ => StubCacheFile::new(asset),
                },
                Err(_) => StubCacheFile::new(asset),
            }
        } else {
            StubCacheFile::new(asset)
        };

        let cache = Arc::new(RwLock::new(cache));

        // Store in memory
        {
            let mut loaded = self.loaded.write().unwrap();
            loaded.insert(hash, cache.clone());
        }

        cache
    }

    /// Look up a cached stub
    pub fn lookup(&self, asset: &AssetKey, fqn: &str) -> Option<IndexNode> {
        let cache = self.get_or_create_cache(asset);
        let cache = cache.read().unwrap();

        cache.entries.get(fqn).map(|e| e.to_index_node())
    }

    /// Store a stub in the cache
    pub fn store(&self, asset: &AssetKey, stub: &IndexNode) {
        let fqn = match &stub.id {
            naviscope_api::models::symbol::NodeId::Flat(s) => s.clone(),
            naviscope_api::models::symbol::NodeId::Structured(s) => format!("{:?}", s),
        };

        let cache = self.get_or_create_cache(asset);
        {
            let mut cache = cache.write().unwrap();
            cache.entries.insert(fqn, CachedStub::from_index_node(stub));
        }

        // Persist to disk
        self.save_cache(asset);
    }

    /// Save cache to disk
    fn save_cache(&self, asset: &AssetKey) {
        let hash = asset.hash();
        let loaded = self.loaded.read().unwrap();

        if let Some(cache) = loaded.get(&hash) {
            let cache = cache.read().unwrap();
            let cache_path = self.cache_path(hash);

            if let Ok(bytes) = rmp_serde::to_vec(&*cache) {
                let _ = fs::write(cache_path, bytes);
            }
        }
    }

    /// Clear all cached data
    pub fn clear(&self) -> std::io::Result<()> {
        // Clear in-memory cache
        {
            let mut loaded = self.loaded.write().unwrap();
            loaded.clear();
        }

        // Remove all cache files
        if self.base_dir.exists() {
            for entry in fs::read_dir(&self.base_dir)? {
                let entry = entry?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "stubs")
                    .unwrap_or(false)
                {
                    let _ = fs::remove_file(entry.path());
                }
            }
        }

        Ok(())
    }

    /// Scan all cached assets returning their summaries
    pub fn scan_assets(&self) -> Vec<CachedAssetSummary> {
        let mut summaries = Vec::new();

        if !self.base_dir.exists() {
            return summaries;
        }

        if let Ok(entries) = fs::read_dir(&self.base_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |ext| ext == "stubs") {
                    if let Ok(metadata) = fs::metadata(&path) {
                        // Try to read file header
                        // Note: We read the whole file for now.
                        // Optimization: Define a Header-only struct for rmp-serde if needed.
                        if let Ok(bytes) = fs::read(&path) {
                            if let Ok(file) = rmp_serde::from_slice::<StubCacheFile>(&bytes) {
                                summaries.push(CachedAssetSummary {
                                    hash: format!("{:016x}", file.asset_hash),
                                    path: file.asset_path,
                                    size_bytes: metadata.len(),
                                    stub_count: file.entries.len(),
                                    version: file.version,
                                    created_at: file.created_at,
                                });
                            }
                        }
                    }
                }
            }
        }

        summaries
    }

    /// Inspect a specific cached asset by hash (full or prefix)
    pub fn inspect_asset(&self, hash_prefix: &str) -> Option<CacheInspectResult> {
        let summaries = self.scan_assets();
        let target = summaries.iter().find(|s| s.hash.starts_with(hash_prefix))?;

        // Convert hex string back to u64 for file path lookup
        let hash = u64::from_str_radix(&target.hash, 16).ok()?;
        let cache_path = self.cache_path(hash);

        if let Ok(bytes) = fs::read(&cache_path) {
            if let Ok(file) = rmp_serde::from_slice::<StubCacheFile>(&bytes) {
                let mut distro = HashMap::new();
                let mut samples = Vec::new();

                for (i, entry) in file.entries.values().enumerate() {
                    *distro.entry(entry.metadata.type_tag.clone()).or_insert(0) += 1;
                    if i < 10 {
                        samples.push(entry.fqn.clone());
                    }
                }

                return Some(CacheInspectResult {
                    summary: CachedAssetSummary {
                        hash: target.hash.clone(),
                        path: file.asset_path,
                        size_bytes: target.size_bytes,
                        stub_count: file.entries.len(),
                        version: file.version,
                        created_at: file.created_at,
                    },
                    metadata_distribution: distro,
                    sample_entries: samples,
                });
            }
        }

        None
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let summaries = self.scan_assets();
        let total_assets = summaries.len();
        let total_entries = summaries.iter().map(|s| s.stub_count).sum();

        CacheStats {
            total_assets,
            total_entries,
            cache_dir: self.base_dir.clone(),
        }
    }
}
