//! Core indexing engine with MVCC support

use crate::asset::service::AssetStubService;
use crate::error::{NaviscopeError, Result};
use crate::indexing::scanner::Scanner;
use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp};
use naviscope_plugin::{
    AssetDiscoverer, AssetIndexer, AssetSourceLocator, BuildCaps, LanguageCaps, NamingConvention,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tokio::sync::RwLock;
use xxhash_rust::xxh3::xxh3_64;

mod lifecycle;
mod storage;
mod watch;

pub const DEFAULT_INDEX_DIR: &str = ".naviscope/indices";

/// Naviscope indexing engine
///
/// Manages the current version of the code graph using MVCC:
/// - Readers get cheap snapshots (Arc clone)
/// - Writers create new versions and atomically swap
/// - No blocking during index updates
pub struct NaviscopeEngine {
    /// Current version of the graph (double Arc for MVCC)
    current: Arc<RwLock<Arc<CodeGraph>>>,

    /// Project root path
    project_root: PathBuf,

    /// Index storage path
    index_path: PathBuf,

    /// Registered capabilities
    build_caps: Arc<Vec<BuildCaps>>,
    lang_caps: Arc<Vec<LanguageCaps>>,

    /// Runtime registry: language name -> naming convention
    naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,

    /// Cancellation token for background tasks (like watcher)
    cancel_token: tokio_util::sync::CancellationToken,

    /// Global stub cache
    stub_cache: Arc<crate::cache::GlobalStubCache>,

    /// Global asset service (new architecture)
    asset_service: Option<Arc<AssetStubService>>,

    /// Resident ingest runtime for source/stub indexing.
    ingest_adapter: tokio::sync::OnceCell<Arc<crate::ingest::IngestAdapter>>,

    /// Stub requests captured before ingest runtime is initialized.
    pending_stub_requests: Mutex<Vec<StubRequest>>,
}

pub struct NaviscopeEngineBuilder {
    project_root: PathBuf,
    build_caps: Vec<BuildCaps>,
    lang_caps: Vec<LanguageCaps>,
}

impl NaviscopeEngineBuilder {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            project_root,
            build_caps: Vec::new(),
            lang_caps: Vec::new(),
        }
    }

    pub fn with_language_caps(mut self, caps: LanguageCaps) -> Self {
        self.lang_caps.push(caps);
        self
    }

    pub fn with_build_caps(mut self, caps: BuildCaps) -> Self {
        self.build_caps.push(caps);
        self
    }

    pub fn build(self) -> NaviscopeEngine {
        let canonical_root = self
            .project_root
            .canonicalize()
            .unwrap_or_else(|_| self.project_root.clone());
        let index_path = NaviscopeEngine::compute_index_path(&canonical_root);
        let cancel_token = tokio_util::sync::CancellationToken::new();
        // Initialize global cache once
        let stub_cache = Arc::new(crate::cache::GlobalStubCache::at_default_location());

        // Process naming conventions
        let mut conventions = HashMap::new();
        for caps in &self.lang_caps {
            if let Some(nc) = caps.presentation.naming_convention() {
                conventions.insert(caps.language.to_string(), nc);
            }
        }

        // Collect asset indexers from language plugins
        let indexers: Vec<Arc<dyn AssetIndexer>> = self
            .lang_caps
            .iter()
            .filter_map(|c| c.asset.asset_indexer())
            .collect();

        // Collect asset discoverers from all plugins
        let mut discoverers: Vec<Box<dyn AssetDiscoverer>> = Vec::new();

        // From language plugins (e.g., JdkDiscoverer from Java)
        for caps in &self.lang_caps {
            if let Some(d) = caps.asset.global_asset_discoverer() {
                discoverers.push(d);
            }
        }

        // From build tool plugins (e.g., GradleCacheDiscoverer from Gradle)
        for caps in &self.build_caps {
            if let Some(d) = caps.asset.global_asset_discoverer() {
                discoverers.push(d);
            }
        }

        // Collect asset source locators from all plugins
        let mut source_locators: Vec<Arc<dyn AssetSourceLocator>> = Vec::new();
        for caps in &self.lang_caps {
            if let Some(locator) = caps.asset.asset_source_locator() {
                source_locators.push(locator);
            }
        }
        for caps in &self.build_caps {
            if let Some(locator) = caps.asset.asset_source_locator() {
                source_locators.push(locator);
            }
        }

        // Project-local asset discoverers (optional hook)
        for caps in &self.lang_caps {
            if let Some(d) = caps.asset.project_asset_discoverer(&canonical_root) {
                discoverers.push(d);
            }
        }

        for caps in &self.build_caps {
            if let Some(d) = caps.asset.project_asset_discoverer(&canonical_root) {
                discoverers.push(d);
            }
        }

        // Create asset service with discoverers from plugins
        let asset_service = if !indexers.is_empty() && !discoverers.is_empty() {
            Some(Arc::new(AssetStubService::new(
                discoverers,
                indexers,
                vec![], // Generators will be added later
                source_locators,
            )))
        } else {
            None
        };

        NaviscopeEngine {
            current: Arc::new(RwLock::new(Arc::new(CodeGraph::empty()))),
            project_root: canonical_root,
            index_path,
            build_caps: Arc::new(self.build_caps),
            lang_caps: Arc::new(self.lang_caps),
            naming_conventions: Arc::new(conventions),
            cancel_token,
            stub_cache,
            asset_service,
            ingest_adapter: tokio::sync::OnceCell::const_new(),
            pending_stub_requests: Mutex::new(Vec::new()),
        }
    }
}

impl Drop for NaviscopeEngine {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl NaviscopeEngine {
    /// Create a builder for the engine
    pub fn builder(project_root: PathBuf) -> NaviscopeEngineBuilder {
        NaviscopeEngineBuilder::new(project_root)
    }

    /// Get the project root path
    pub fn root_path(&self) -> &Path {
        &self.project_root
    }

    /// Query semantic capabilities for a language.
    pub fn semantic_cap(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::SemanticCap>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .map(|c| c.semantic.clone())
    }

    /// Query node presenter for language or matching build-tool capability.
    pub fn node_presenter(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::NodePresenter>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .and_then(|c| c.presentation.node_presenter())
            .or_else(|| {
                self.build_caps
                    .iter()
                    .find(|c| c.build_tool.as_str() == language.as_str())
                    .and_then(|c| c.presentation.node_presenter())
            })
    }

    /// Query metadata codec for language or matching build-tool capability.
    pub fn metadata_codec(
        &self,
        language: crate::model::source::Language,
    ) -> Option<Arc<dyn naviscope_plugin::NodeMetadataCodec>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .and_then(|c| c.metadata_codec.metadata_codec())
            .or_else(|| {
                self.build_caps
                    .iter()
                    .find(|c| c.build_tool.as_str() == language.as_str())
                    .and_then(|c| c.metadata_codec.metadata_codec())
            })
    }

    /// Detect language capability by path.
    pub fn language_for_path(
        &self,
        path: &std::path::Path,
    ) -> Option<crate::model::source::Language> {
        self.lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(path))
            .map(|c| c.language.clone())
    }

    /// Get naming conventions registry (cheap Arc clone)
    pub(crate) fn naming_conventions(
        &self,
    ) -> Arc<std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>> {
        self.naming_conventions.clone()
    }

    /// Get the asset service (if available)
    pub fn asset_service(&self) -> Option<&Arc<AssetStubService>> {
        self.asset_service.as_ref()
    }

    /// Request on-demand stub generation for a single FQN.
    /// Returns true if a request was accepted for execution.
    pub fn request_stub_for_fqn(&self, fqn: &str) -> bool {
        let Some(service) = &self.asset_service else {
            return false;
        };
        let Some(candidate_paths) = service.lookup_paths(fqn) else {
            return false;
        };
        if candidate_paths.is_empty() {
            return false;
        }

        let req = StubRequest {
            fqn: fqn.to_string(),
            candidate_paths,
        };

        if let Some(runtime) = self.ingest_adapter.get() {
            return runtime.try_submit_stub_request(req).is_ok();
        }

        if let Ok(mut pending) = self.pending_stub_requests.lock() {
            pending.push(req);
            return true;
        }

        false
    }

    /// Run the global asset scan and populate routes
    /// Returns the scan result with statistics
    pub async fn scan_global_assets(&self) -> Option<crate::asset::scanner::ScanResult> {
        if let Some(service) = &self.asset_service {
            let service = service.clone();
            let result = tokio::task::spawn_blocking(move || service.scan_sync())
                .await
                .ok();
            result
        } else {
            None
        }
    }

    /// Get global asset routes snapshot (for passing to resolvers)
    pub fn global_asset_routes(&self) -> HashMap<String, Vec<PathBuf>> {
        if let Some(service) = &self.asset_service {
            service.routes_snapshot()
        } else {
            HashMap::new()
        }
    }

    /// Compute index storage path for a project
    fn compute_index_path(project_root: &Path) -> PathBuf {
        let base_dir = Self::get_base_index_dir();
        let abs_path = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.to_path_buf());
        let hash = xxh3_64(abs_path.to_string_lossy().as_bytes());
        base_dir.join(format!("{:016x}.bin", hash))
    }

    /// Get a snapshot of the current graph (cheap operation)
    pub async fn snapshot(&self) -> CodeGraph {
        let lock = self.current.read().await;
        (**lock).clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_snapshot_is_fast() {
        let engine = NaviscopeEngine::builder(PathBuf::from(".")).build();

        let start = std::time::Instant::now();
        for _ in 0..1000 {
            let _graph = engine.snapshot().await;
        }
        let elapsed = start.elapsed();

        // 1000 snapshots should be very fast
        assert!(elapsed.as_millis() < 100, "Snapshots should be fast");
    }

    #[tokio::test]
    async fn test_concurrent_snapshots() {
        use tokio::task::JoinSet;

        let engine = Arc::new(NaviscopeEngine::builder(PathBuf::from(".")).build());

        let mut set = JoinSet::new();

        for _ in 0..10 {
            let e = Arc::clone(&engine);
            set.spawn(async move {
                for _ in 0..10 {
                    let graph = e.snapshot().await;
                    assert_eq!(graph.node_count(), 0);
                }
            });
        }

        while let Some(result) = set.join_next().await {
            result.unwrap();
        }
    }
}
