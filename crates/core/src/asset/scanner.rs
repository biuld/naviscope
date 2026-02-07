//! Asset scanner that combines multiple discoverers and indexers.
//!
//! The scanner orchestrates the discovery and indexing pipeline:
//! 1. Discoverers find asset files (JAR, jimage, etc.)
//! 2. Indexers extract package prefixes from each asset
//! 3. Results are registered in the route registry

use naviscope_plugin::{AssetDiscoverer, AssetEntry, AssetIndexer, AssetRouteRegistry};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Combines multiple asset discoverers with indexers
pub struct AssetScanner {
    discoverers: Vec<Box<dyn AssetDiscoverer>>,
    indexers: Vec<Arc<dyn AssetIndexer>>,
}

impl AssetScanner {
    pub fn new() -> Self {
        Self {
            discoverers: Vec::new(),
            indexers: Vec::new(),
        }
    }

    /// Add a discoverer
    pub fn add_discoverer(mut self, discoverer: Box<dyn AssetDiscoverer>) -> Self {
        self.discoverers.push(discoverer);
        self
    }

    /// Add an indexer
    pub fn add_indexer(mut self, indexer: Arc<dyn AssetIndexer>) -> Self {
        self.indexers.push(indexer);
        self
    }

    /// Add multiple discoverers
    pub fn with_discoverers(
        mut self,
        discoverers: impl IntoIterator<Item = Box<dyn AssetDiscoverer>>,
    ) -> Self {
        self.discoverers.extend(discoverers);
        self
    }

    /// Add multiple indexers
    pub fn with_indexers(
        mut self,
        indexers: impl IntoIterator<Item = Arc<dyn AssetIndexer>>,
    ) -> Self {
        self.indexers.extend(indexers);
        self
    }

    /// Scan all assets and register routes
    ///
    /// This method streams through assets to maintain constant memory usage:
    /// - Discoverers yield assets one at a time
    /// - Indexers extract prefixes and immediately register them
    pub fn scan(&self, registry: &dyn AssetRouteRegistry) -> ScanResult {
        let mut result = ScanResult::default();
        let start = std::time::Instant::now();

        for discoverer in &self.discoverers {
            debug!("Scanning with discoverer: {}", discoverer.name());
            let discoverer_start = std::time::Instant::now();
            let mut discoverer_assets = 0;

            for entry in discoverer.discover() {
                discoverer_assets += 1;
                result.total_assets += 1;

                // Find a suitable indexer
                if let Some(indexer) = self.find_indexer(&entry) {
                    match self.index_and_register(&entry, indexer.as_ref(), registry) {
                        Ok(prefix_count) => {
                            result.indexed_assets += 1;
                            result.total_prefixes += prefix_count;
                        }
                        Err(e) => {
                            warn!("Failed to index {:?}: {}", entry.path, e);
                            result.failed_assets += 1;
                        }
                    }
                } else {
                    debug!("No indexer for {:?}", entry.path);
                    result.skipped_assets += 1;
                }
            }

            debug!(
                "Discoverer {} found {} assets in {:?}",
                discoverer.name(),
                discoverer_assets,
                discoverer_start.elapsed()
            );
        }

        result.duration = start.elapsed();
        info!(
            "Asset scan complete: {} assets, {} indexed, {} prefixes in {:?}",
            result.total_assets, result.indexed_assets, result.total_prefixes, result.duration
        );

        result
    }

    /// Find an indexer that can handle the given asset
    fn find_indexer(&self, entry: &AssetEntry) -> Option<Arc<dyn AssetIndexer>> {
        self.indexers
            .iter()
            .find(|indexer| indexer.can_index(&entry.path))
            .cloned()
    }

    /// Index an asset and register all discovered prefixes
    fn index_and_register(
        &self,
        entry: &AssetEntry,
        indexer: &dyn AssetIndexer,
        registry: &dyn AssetRouteRegistry,
    ) -> Result<usize, Box<dyn std::error::Error + Send + Sync>> {
        let prefixes = indexer.index(&entry.path)?;
        let count = prefixes.len();

        for prefix in prefixes {
            registry.register(prefix, entry.clone());
        }

        Ok(count)
    }
}

impl Default for AssetScanner {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of an asset scan operation
#[derive(Debug, Default, Clone)]
pub struct ScanResult {
    /// Total number of assets discovered
    pub total_assets: usize,
    /// Number of assets successfully indexed
    pub indexed_assets: usize,
    /// Number of assets skipped (no indexer available)
    pub skipped_assets: usize,
    /// Number of assets that failed to index
    pub failed_assets: usize,
    /// Total number of prefixes registered
    pub total_prefixes: usize,
    /// Time taken for the scan
    pub duration: std::time::Duration,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asset::registry::InMemoryRouteRegistry;
    use naviscope_plugin::{AssetSource, BoxError};
    use std::path::{Path, PathBuf};

    /// Mock discoverer for testing
    struct MockDiscoverer {
        assets: Vec<AssetEntry>,
    }

    impl AssetDiscoverer for MockDiscoverer {
        fn discover(&self) -> Box<dyn Iterator<Item = AssetEntry> + Send + '_> {
            Box::new(self.assets.iter().cloned())
        }

        fn name(&self) -> &str {
            "Mock Discoverer"
        }

        fn source_type(&self) -> &str {
            "mock"
        }
    }

    /// Mock indexer for testing
    struct MockIndexer {
        prefixes_per_asset: Vec<String>,
    }

    impl AssetIndexer for MockIndexer {
        fn can_index(&self, _asset: &Path) -> bool {
            true
        }

        fn index(&self, _asset: &Path) -> Result<Vec<String>, BoxError> {
            Ok(self.prefixes_per_asset.clone())
        }
    }

    #[test]
    fn test_scanner_basic() {
        let discoverer = Box::new(MockDiscoverer {
            assets: vec![
                AssetEntry::new(PathBuf::from("/test1.jar"), AssetSource::Unknown),
                AssetEntry::new(PathBuf::from("/test2.jar"), AssetSource::Unknown),
            ],
        });

        let indexer = Arc::new(MockIndexer {
            prefixes_per_asset: vec!["com.example".to_string(), "org.test".to_string()],
        });

        let scanner = AssetScanner::new()
            .add_discoverer(discoverer)
            .add_indexer(indexer);

        let registry = InMemoryRouteRegistry::new();
        let result = scanner.scan(&registry);

        assert_eq!(result.total_assets, 2);
        assert_eq!(result.indexed_assets, 2);
        assert_eq!(result.total_prefixes, 4); // 2 prefixes × 2 assets

        // Verify registry contents
        let stats = registry.stats();
        assert_eq!(stats.total_prefixes, 2); // "com.example" and "org.test"
        assert_eq!(stats.total_entries, 4); // Each prefix has 2 entries (from 2 assets)
    }

    #[test]
    fn test_scanner_no_indexer() {
        let discoverer = Box::new(MockDiscoverer {
            assets: vec![AssetEntry::new(
                PathBuf::from("/test.jar"),
                AssetSource::Unknown,
            )],
        });

        // No indexer added
        let scanner = AssetScanner::new().add_discoverer(discoverer);

        let registry = InMemoryRouteRegistry::new();
        let result = scanner.scan(&registry);

        assert_eq!(result.total_assets, 1);
        assert_eq!(result.skipped_assets, 1);
        assert_eq!(result.indexed_assets, 0);
    }
}
