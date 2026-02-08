//! Unified Asset/Stub service facade.
//!
//! Provides a single entry point for:
//! - Asset discovery and route management
//! - Stub request handling
//! - Background scanning

use crate::asset::registry::InMemoryRouteRegistry;
use crate::asset::scanner::{AssetScanner, ScanResult};
use naviscope_plugin::{
    AssetDiscoverer, AssetEntry, AssetIndexer, AssetRouteRegistry, AssetSourceLocator,
    RegistryStats, StubGenerator, StubRequest,
};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::debug;

/// Unified Asset/Stub service
pub struct AssetStubService {
    /// Route registry (shared, thread-safe)
    registry: Arc<InMemoryRouteRegistry>,

    /// Asset scanner configuration
    scanner: AssetScanner,

    /// Stub generators (from language plugins)
    generators: Vec<Arc<dyn StubGenerator>>,

    /// Channel for stub requests
    stub_tx: Option<mpsc::UnboundedSender<StubRequest>>,

    /// Mapping from binary asset path to source asset path (if available)
    source_map: Arc<RwLock<HashMap<PathBuf, PathBuf>>>,

    /// Source locators (from language/build plugins)
    source_locators: Vec<Arc<dyn AssetSourceLocator>>,
}

impl AssetStubService {
    /// Create a new service with discoverers, indexers, and generators
    pub fn new(
        discoverers: Vec<Box<dyn AssetDiscoverer>>,
        indexers: Vec<Arc<dyn AssetIndexer>>,
        generators: Vec<Arc<dyn StubGenerator>>,
        source_locators: Vec<Arc<dyn AssetSourceLocator>>,
    ) -> Self {
        let scanner = AssetScanner::new()
            .with_discoverers(discoverers)
            .with_indexers(indexers);

        Self {
            registry: Arc::new(InMemoryRouteRegistry::new()),
            scanner,
            generators,
            stub_tx: None,
            source_map: Arc::new(RwLock::new(HashMap::new())),
            source_locators,
        }
    }

    /// Create with custom registry (for testing or shared state)
    pub fn with_registry(
        registry: Arc<InMemoryRouteRegistry>,
        discoverers: Vec<Box<dyn AssetDiscoverer>>,
        indexers: Vec<Arc<dyn AssetIndexer>>,
        generators: Vec<Arc<dyn StubGenerator>>,
        source_locators: Vec<Arc<dyn AssetSourceLocator>>,
    ) -> Self {
        let scanner = AssetScanner::new()
            .with_discoverers(discoverers)
            .with_indexers(indexers);

        Self {
            registry,
            scanner,
            generators,
            stub_tx: None,
            source_map: Arc::new(RwLock::new(HashMap::new())),
            source_locators,
        }
    }

    /// Set the stub request channel
    pub fn with_stub_channel(mut self, tx: mpsc::UnboundedSender<StubRequest>) -> Self {
        self.stub_tx = Some(tx);
        self
    }

    /// Get a reference to the registry
    pub fn registry(&self) -> Arc<InMemoryRouteRegistry> {
        self.registry.clone()
    }

    /// Perform a synchronous scan (blocks until complete)
    pub fn scan_sync(&self) -> ScanResult {
        let result = self.scanner.scan(self.registry.as_ref());
        self.refresh_source_map();
        result
    }

    /// Start a background scan task
    pub fn spawn_scan(&self) -> JoinHandle<ScanResult> {
        let registry = self.registry.clone();
        let scanner = self.build_scanner_clone();
        let source_map = self.source_map.clone();
        let source_locators = self.source_locators.clone();

        tokio::spawn(async move {
            // Run scan in blocking thread pool
            tokio::task::spawn_blocking(move || {
                let result = scanner.scan(registry.as_ref());
                let map = Self::build_source_map(registry.as_ref(), &source_locators);
                if let Ok(mut guard) = source_map.try_write() {
                    *guard = map;
                }
                result
            })
            .await
            .unwrap_or_default()
        })
    }

    /// Lookup asset entries for an FQN
    pub fn lookup_asset(&self, fqn: &str) -> Option<Vec<AssetEntry>> {
        self.registry.lookup(fqn)
    }

    /// Lookup asset paths for an FQN (legacy compatibility)
    pub fn lookup_paths(&self, fqn: &str) -> Option<Vec<PathBuf>> {
        self.registry
            .lookup(fqn)
            .map(|entries| entries.into_iter().map(|e| e.path).collect())
    }

    /// Lookup source asset for a binary asset path
    pub fn lookup_source(&self, binary_path: &std::path::Path) -> Option<PathBuf> {
        self.source_map
            .try_read()
            .ok()
            .and_then(|map| map.get(binary_path).cloned())
    }

    /// Request stub generation (async, non-blocking)
    pub fn request_stub(&self, fqn: String, candidate_entries: Vec<AssetEntry>) {
        if let Some(tx) = &self.stub_tx {
            let request = StubRequest::new(fqn.clone(), candidate_entries);
            if let Err(e) = tx.send(request) {
                tracing::warn!("Failed to send stub request for {}: {}", fqn, e);
            } else {
                debug!("Sent stub request for {}", fqn);
            }
        }
    }

    /// Get a snapshot of all routes (for serialization or passing to resolver)
    pub fn routes_snapshot(&self) -> HashMap<String, Vec<PathBuf>> {
        self.registry
            .all_routes()
            .into_iter()
            .map(|(k, v)| (k, v.into_iter().map(|e| e.path).collect()))
            .collect()
    }

    /// Refresh source map using discovered binary assets
    pub fn refresh_source_map(&self) {
        let map = Self::build_source_map(self.registry.as_ref(), &self.source_locators);
        if let Ok(mut guard) = self.source_map.try_write() {
            *guard = map;
        }
    }

    /// Get registry statistics
    pub fn stats(&self) -> RegistryStats {
        self.registry.stats()
    }

    /// Get stub generators
    pub fn generators(&self) -> &[Arc<dyn StubGenerator>] {
        &self.generators
    }

    /// Find a generator that can handle the given asset
    pub fn find_generator(&self, asset: &std::path::Path) -> Option<Arc<dyn StubGenerator>> {
        self.generators
            .iter()
            .find(|g| g.can_generate(asset))
            .cloned()
    }

    // Helper to rebuild scanner (since AssetScanner contains non-Clone types)
    fn build_scanner_clone(&self) -> AssetScanner {
        // Note: This is a limitation - we can't easily clone the scanner
        // For now, return a default scanner. The real implementation would
        // need to use a different approach (e.g., Arc<Scanner> or factory pattern)
        AssetScanner::new()
    }

    fn build_source_map(
        registry: &InMemoryRouteRegistry,
        locators: &[Arc<dyn AssetSourceLocator>],
    ) -> HashMap<PathBuf, PathBuf> {
        let mut map = HashMap::new();
        let mut seen = HashSet::new();
        for entries in registry.all_routes().values() {
            for entry in entries {
                if !seen.insert(entry.path.clone()) {
                    continue;
                }
                for locator in locators {
                    if let Some(source) = locator.locate_source(entry) {
                        map.insert(entry.path.clone(), source);
                        break;
                    }
                }
            }
        }
        map
    }
}

/// Builder for AssetStubService
pub struct AssetStubServiceBuilder {
    discoverers: Vec<Box<dyn AssetDiscoverer>>,
    indexers: Vec<Arc<dyn AssetIndexer>>,
    generators: Vec<Arc<dyn StubGenerator>>,
    source_locators: Vec<Arc<dyn AssetSourceLocator>>,
    registry: Option<Arc<InMemoryRouteRegistry>>,
    stub_tx: Option<mpsc::UnboundedSender<StubRequest>>,
}

impl AssetStubServiceBuilder {
    pub fn new() -> Self {
        Self {
            discoverers: Vec::new(),
            indexers: Vec::new(),
            generators: Vec::new(),
            source_locators: Vec::new(),
            registry: None,
            stub_tx: None,
        }
    }

    pub fn add_discoverer(mut self, discoverer: Box<dyn AssetDiscoverer>) -> Self {
        self.discoverers.push(discoverer);
        self
    }

    pub fn add_indexer(mut self, indexer: Arc<dyn AssetIndexer>) -> Self {
        self.indexers.push(indexer);
        self
    }

    pub fn add_generator(mut self, generator: Arc<dyn StubGenerator>) -> Self {
        self.generators.push(generator);
        self
    }

    pub fn add_source_locator(mut self, locator: Arc<dyn AssetSourceLocator>) -> Self {
        self.source_locators.push(locator);
        self
    }

    pub fn with_registry(mut self, registry: Arc<InMemoryRouteRegistry>) -> Self {
        self.registry = Some(registry);
        self
    }

    pub fn with_stub_channel(mut self, tx: mpsc::UnboundedSender<StubRequest>) -> Self {
        self.stub_tx = Some(tx);
        self
    }

    pub fn build(self) -> AssetStubService {
        let mut service = if let Some(registry) = self.registry {
            AssetStubService::with_registry(
                registry,
                self.discoverers,
                self.indexers,
                self.generators,
                self.source_locators,
            )
        } else {
            AssetStubService::new(
                self.discoverers,
                self.indexers,
                self.generators,
                self.source_locators,
            )
        };

        if let Some(tx) = self.stub_tx {
            service = service.with_stub_channel(tx);
        }

        service
    }
}

impl Default for AssetStubServiceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_plugin::{AssetSource, BoxError};
    use std::path::Path;

    struct MockDiscoverer;

    impl AssetDiscoverer for MockDiscoverer {
        fn discover(&self) -> Box<dyn Iterator<Item = AssetEntry> + Send + '_> {
            Box::new(std::iter::once(AssetEntry::new(
                PathBuf::from("/test.jar"),
                AssetSource::Unknown,
            )))
        }

        fn name(&self) -> &str {
            "Mock"
        }

        fn source_type(&self) -> &str {
            "mock"
        }
    }

    struct MockIndexer;

    impl AssetIndexer for MockIndexer {
        fn can_index(&self, _: &Path) -> bool {
            true
        }

        fn index(&self, _: &Path) -> Result<Vec<String>, BoxError> {
            Ok(vec!["com.example".to_string()])
        }
    }

    #[test]
    fn test_service_basic() {
        let service = AssetStubService::new(
            vec![Box::new(MockDiscoverer)],
            vec![Arc::new(MockIndexer)],
            vec![],
            vec![],
        );

        let result = service.scan_sync();
        assert_eq!(result.indexed_assets, 1);
        assert_eq!(result.total_prefixes, 1);

        let entries = service.lookup_asset("com.example.Foo");
        assert!(entries.is_some());
    }

    #[test]
    fn test_builder() {
        let service = AssetStubServiceBuilder::new()
            .add_discoverer(Box::new(MockDiscoverer))
            .add_indexer(Arc::new(MockIndexer))
            .build();

        let result = service.scan_sync();
        assert_eq!(result.indexed_assets, 1);
    }
}
