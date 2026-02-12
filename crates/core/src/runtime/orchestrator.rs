//! Core indexing engine with MVCC support

use crate::asset::service::AssetStubService;
use crate::error::{NaviscopeError, Result};
use crate::ingest::builder::CodeGraphBuilder;
use crate::ingest::resolver::{IndexResolver, StubRequest, StubbingManager};
use crate::ingest::scanner::Scanner;
use crate::model::CodeGraph;
use crate::model::GraphOp;
use naviscope_plugin::{
    AssetDiscoverer, AssetEntry, AssetIndexer, AssetSource, AssetSourceLocator, BuildCaps,
    LanguageCaps, NamingConvention,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use xxhash_rust::xxh3::xxh3_64;

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

    /// Background stubbing channel
    stub_tx: tokio::sync::mpsc::UnboundedSender<StubRequest>,

    /// Global stub cache
    stub_cache: Arc<crate::cache::GlobalStubCache>,

    /// Global asset service (new architecture)
    asset_service: Option<Arc<AssetStubService>>,
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

        let (stub_tx, stub_rx) = tokio::sync::mpsc::unbounded_channel();
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

        let engine = NaviscopeEngine {
            current: Arc::new(RwLock::new(Arc::new(CodeGraph::empty()))),
            project_root: canonical_root,
            index_path,
            build_caps: Arc::new(self.build_caps),
            lang_caps: Arc::new(self.lang_caps),
            naming_conventions: Arc::new(conventions),
            cancel_token: cancel_token.clone(),
            stub_tx,
            stub_cache: stub_cache.clone(),
            asset_service,
        };

        engine.spawn_stub_worker(stub_rx, cancel_token, stub_cache);

        engine
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

    // ... helper methods ...

    /// Get the project root path
    pub fn root_path(&self) -> &Path {
        &self.project_root
    }

    /// Get the index resolver configured with current plugins
    pub fn get_resolver(&self) -> IndexResolver {
        IndexResolver::with_caps((*self.build_caps).clone(), (*self.lang_caps).clone())
            .with_stubbing(StubbingManager::new(self.stub_tx.clone()))
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
    /// Returns true if a request was successfully enqueued.
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
        self.stub_tx
            .send(StubRequest {
                fqn: fqn.to_string(),
                candidate_paths,
            })
            .is_ok()
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
        (**lock).clone() // CodeGraph clone is Arc clone of inner
    }

    /// Load index from disk
    pub async fn load(&self) -> Result<bool> {
        let path = self.index_path.clone();
        let lang_caps = self.lang_caps.clone();
        let build_caps = self.build_caps.clone();

        // Load in blocking pool
        let graph_opt =
            tokio::task::spawn_blocking(move || Self::load_from_disk(&path, lang_caps, build_caps))
                .await
                .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        if let Some(graph) = graph_opt {
            // Atomically update current
            let mut lock = self.current.write().await;
            *lock = Arc::new(graph);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Save current graph to disk
    pub async fn save(&self) -> Result<()> {
        let graph = self.snapshot().await;
        let path = self.index_path.clone();
        let lang_caps = self.lang_caps.clone();
        let build_caps = self.build_caps.clone();

        tokio::task::spawn_blocking(move || {
            Self::save_to_disk(&graph, &path, lang_caps, build_caps)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?
    }

    /// Rebuild the index from scratch
    pub async fn rebuild(&self) -> Result<()> {
        let _ = self.scan_global_assets().await;
        let project_root = self.project_root.clone();
        let build_caps = self.build_caps.clone();
        let lang_caps = self.lang_caps.clone();
        let global_routes = self.global_asset_routes();

        let stub_tx = self.stub_tx.clone();
        let (new_graph, stubs) = tokio::task::spawn_blocking(move || {
            Self::build_index(&project_root, build_caps, lang_caps, stub_tx, global_routes)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        // Atomically update (write lock held for microseconds)
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(new_graph);
        }

        // Schedule stubs AFTER graph update using explicit requests
        for req in stubs {
            // We use a fresh stub_tx here, actually we need access to self.stub_tx?
            // self.stub_tx is async channel. send is async or blocking?
            // UnboundedSender::send is non-blocking.
            if let Err(e) = self.stub_tx.send(req.clone()) {
                tracing::warn!("Failed to schedule stub: {}", e);
            }
        }

        // Save to disk
        self.save().await?;

        Ok(())
    }

    /// Update specific files incrementally
    pub async fn update_files(&self, files: Vec<PathBuf>) -> Result<()> {
        let _ = self.scan_global_assets().await;
        let base_graph = self.snapshot().await;
        let build_caps = self.build_caps.clone();
        let lang_caps = self.lang_caps.clone();
        let global_routes = Arc::new(self.global_asset_routes());

        // Prepare existing file metadata for change detection
        let mut existing_metadata = std::collections::HashMap::new();
        for (path, entry) in base_graph.file_index() {
            existing_metadata.insert(
                PathBuf::from(base_graph.symbols().resolve(&path.0)),
                entry.metadata.clone(),
            );
        }

        let current_lock = self.current.clone();
        let stub_tx = self.stub_tx.clone();

        // Processing in blocking pool
        tokio::task::spawn_blocking(move || -> Result<()> {
            let mut manual_ops = Vec::new();
            let mut to_scan = Vec::new();

            for path in files {
                if path.exists() {
                    to_scan.push(path);
                } else {
                    // File was deleted
                    manual_ops.push(GraphOp::RemovePath {
                        path: Arc::from(path.as_path()),
                    });
                }
            }

            // 1. Initial scan to identify file types and changes
            let scan_results = Scanner::scan_files(to_scan, &existing_metadata);
            if scan_results.is_empty() && manual_ops.is_empty() {
                return Ok(());
            }

            // Partition into build and source
            let (build_files, source_files): (Vec<_>, Vec<_>) =
                scan_results.into_iter().partition(|f| f.is_build());

            let resolver = IndexResolver::with_caps((*build_caps).clone(), (*lang_caps).clone())
                .with_stubbing(StubbingManager::new(stub_tx.clone()));

            // 2. Phase 1: Heavy Build Resolution (Global Context)
            let mut project_context_inner = crate::ingest::resolver::ProjectContext::new();
            let mut initial_ops = manual_ops;

            // IMPORTANT: RemovePath MUST come before AddNode for the same paths.
            // Add RemovePath and UpdateFile for build files up front.
            for bf in &build_files {
                initial_ops.push(GraphOp::RemovePath {
                    path: Arc::from(bf.path()),
                });
                initial_ops.push(GraphOp::UpdateFile {
                    metadata: bf.file.clone(),
                });
            }

            // For build files, we still process them up front because they define the structure
            let build_ops =
                resolver.resolve_build_batch(&build_files, &mut project_context_inner)?;
            initial_ops.extend(build_ops);

            let project_context = Arc::new(project_context_inner);
            let routes = global_routes.clone();

            // 3. Phase 2: Pipeline Batch Processing for source files
            let pipeline = crate::ingest::pipeline::IngestPipeline::new(500); // 500 files per batch
            let source_paths: Vec<PathBuf> = source_files
                .into_iter()
                .map(|f| f.path().to_path_buf())
                .collect();

            let mut builder = base_graph.to_builder();

            // Register naming conventions
            for caps in lang_caps.iter() {
                if let Some(nc) = caps.presentation.naming_convention() {
                    builder.naming_conventions.insert(caps.language.clone(), nc);
                }
            }

            builder.apply_ops(initial_ops)?;

            let mut pending_stubs = Vec::new();
            // Note: We are in a blocking thread, resolver and context are Thread-safe.
            pipeline.execute(&*project_context, source_paths, &resolver, |batch_ops| {
                builder.apply_ops(batch_ops.clone())?;
                let reqs = resolver.resolve_stubs(&batch_ops, routes.as_ref());
                pending_stubs.extend(reqs);
                Ok(())
            })?;

            // 4. Final Swap
            let final_graph = Arc::new(builder.build());
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut lock = current_lock.write().await;
                *lock = final_graph;
            });

            // 5. Schedule stubs
            for req in pending_stubs {
                if let Err(e) = stub_tx.send(req) {
                    tracing::warn!("Failed to schedule stub: {}", e);
                }
            }

            Ok(())
        })
        .await
        .map_err(|e| crate::error::NaviscopeError::Internal(e.to_string()))??;

        // Save at the very end
        self.save().await?;

        Ok(())
    }

    /// Refresh index (detect changes and update)
    pub async fn refresh(&self) -> Result<()> {
        let project_root = self.project_root.clone();

        // Scan for all current files and update incrementally
        let paths = tokio::task::spawn_blocking(move || Scanner::collect_paths(&project_root))
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        self.update_files(paths).await
    }

    /// Watch for filesystem changes and update incrementally.
    /// The watcher task exits when `cancel_token` is cancelled.
    pub async fn start_watch_with_token(
        self: Arc<Self>,
        cancel_token: tokio_util::sync::CancellationToken,
    ) -> Result<()> {
        use crate::runtime::watcher::Watcher;
        use std::collections::HashSet;
        use std::time::Duration;

        let root = self.project_root.clone();
        let mut watcher =
            Watcher::new(&root).map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        let engine_weak = Arc::downgrade(&self);

        tokio::spawn(async move {
            tracing::info!("Started watching {}", root.display());
            let mut pending_events: Vec<notify::Event> = Vec::new();
            let debounce_interval = Duration::from_millis(500);

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    event = watcher.next_event_async() => {
                        match event {
                            Some(e) => pending_events.push(e),
                            None => break,
                        }
                    }
                    _ = tokio::time::sleep(debounce_interval), if !pending_events.is_empty() => {
                        let mut paths = HashSet::new();
                        for event in &pending_events {
                            for path in &event.paths {
                                if crate::ingest::is_relevant_path(path) {
                                    paths.insert(path.clone());
                                }
                            }
                        }
                        pending_events.clear();

                        if !paths.is_empty() {
                            if let Some(engine) = engine_weak.upgrade() {
                                let path_vec: Vec<_> = paths.into_iter().collect();
                                tracing::info!("Detected changes in {} files. Updating...", path_vec.len());
                                if let Err(err) = engine.update_files(path_vec).await {
                                    tracing::error!("Failed to update files: {}", err);
                                }
                            } else {
                                break;
                            }
                        }
                    }
                }
            }
            tracing::info!("File watcher task ended for {}", root.display());
        });

        Ok(())
    }

    /// Backward-compatible helper that uses the engine-wide cancellation token.
    pub async fn watch(self: Arc<Self>) -> Result<()> {
        let cancel_token = self.cancel_token.clone();
        self.start_watch_with_token(cancel_token).await
    }

    /// Start the background stubbing worker
    fn spawn_stub_worker(
        &self,
        mut rx: tokio::sync::mpsc::UnboundedReceiver<StubRequest>,
        cancel_token: tokio_util::sync::CancellationToken,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) {
        let current = self.current.clone();
        let lang_caps = self.lang_caps.clone();
        let naming_conventions = self.naming_conventions.clone();

        tokio::spawn(async move {
            tracing::info!("Stubbing worker started");
            let mut seen_fqns = std::collections::HashSet::new();

            loop {
                tokio::select! {
                    _ = cancel_token.cancelled() => break,
                    Some(req) = rx.recv() => {
                        // Skip if already seen in this session to avoid redundant work
                        if !seen_fqns.insert(req.fqn.clone()) {
                            continue;
                        }

                        // Check if node already exists and is resolved
                        {
                            let lock = current.read().await;
                            let graph = &**lock;
                            if let Some(idx) = graph.find_node(&req.fqn) {
                                if let Some(node) = graph.get_node(idx) {
                                    if node.status == naviscope_api::models::graph::ResolutionStatus::Resolved {
                                        continue;
                                    }
                                }
                            }
                        }

                        // Resolve
                        let mut ops = Vec::new();

                        for asset_path in req.candidate_paths {
                            // Try to create asset key for cache lookup
                            let asset_key = crate::cache::AssetKey::from_path(&asset_path).ok();

                            // Check cache first
                            if let Some(ref key) = asset_key {
                                if let Some(cached_stub) = stub_cache.lookup(key, &req.fqn) {
                                    tracing::trace!("Cache hit for {}", req.fqn);
                                    ops.push(GraphOp::AddNode {
                                        data: Some(cached_stub),
                                    });
                                    break; // Found it
                                }
                            }

                            // If not in cache, generate stub
                            for caps in lang_caps.iter() {
                                let Some(generator) = caps.asset.stub_generator() else {
                                    continue;
                                };
                                if !generator.can_generate(&asset_path) {
                                    continue;
                                }

                                let entry =
                                    AssetEntry::new(asset_path.clone(), AssetSource::Unknown);
                                match generator.generate(&req.fqn, &entry) {
                                    Ok(stub) => {
                                        // Store in cache for future use
                                        if let Some(ref key) = asset_key {
                                            stub_cache.store(key, &stub);
                                            tracing::trace!("Cached stub for {}", req.fqn);
                                        }
                                        ops.push(GraphOp::AddNode { data: Some(stub) });
                                        break;
                                    }
                                    Err(e) => {
                                        tracing::debug!(
                                            "Failed to generate stub for {}: {}",
                                            req.fqn,
                                            e
                                        );
                                    }
                                }
                            }

                            if !ops.is_empty() {
                                break;
                            }
                        }

                        if !ops.is_empty() {
                            let mut lock = current.write().await;
                            let mut builder = (**lock).to_builder();

                            // Load naming conventions
                            let conventions = (*naming_conventions).clone();
                            for (lang, nc) in conventions {
                                builder.naming_conventions.insert(naviscope_api::models::Language::from(lang), nc);
                            }

                            if let Ok(()) = builder.apply_ops(ops) {
                                *lock = Arc::new(builder.build());
                                tracing::trace!("Applied stub for {}", req.fqn);
                            }
                        }
                    }
                }
            }
            tracing::info!("Stubbing worker stopped");
        });
    }

    /// Clear the index for the current project
    pub async fn clear_project_index(&self) -> Result<()> {
        let path = self.index_path.clone();
        if path.exists() {
            tokio::fs::remove_file(path).await?;
        }

        // Reset current graph
        let mut lock = self.current.write().await;
        *lock = Arc::new(CodeGraph::empty());

        Ok(())
    }

    /// Clear all indices
    pub fn clear_all_indices() -> Result<()> {
        let base_dir = Self::get_base_index_dir();
        if base_dir.exists() {
            std::fs::remove_dir_all(&base_dir)?;
        }
        Ok(())
    }

    /// Gets the base directory for storing indices, supporting NAVISCOPE_INDEX_DIR env var.
    pub fn get_base_index_dir() -> PathBuf {
        if let Ok(env_dir) = std::env::var("NAVISCOPE_INDEX_DIR") {
            return PathBuf::from(env_dir);
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(super::DEFAULT_INDEX_DIR)
    }

    // ---- Helper methods ----

    fn load_from_disk(
        path: &Path,
        lang_caps: Arc<Vec<LanguageCaps>>,
        build_caps: Arc<Vec<BuildCaps>>,
    ) -> Result<Option<CodeGraph>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(path)?;

        let get_codec = |lang: &str| -> Option<Arc<dyn crate::bridge::NodeMetadataCodec>> {
            for caps in lang_caps.iter() {
                if caps.language.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            for caps in build_caps.iter() {
                if caps.build_tool.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            None
        };

        match CodeGraph::deserialize(&bytes, get_codec) {
            Ok(graph) => {
                if graph.version() != crate::model::graph::CURRENT_VERSION {
                    tracing::warn!(
                        "Index version mismatch at {} (found {}, expected {}). Will rebuild.",
                        path.display(),
                        graph.version(),
                        crate::model::graph::CURRENT_VERSION
                    );
                    let _ = std::fs::remove_file(path);
                    return Ok(None);
                }
                tracing::info!("Loaded index from {}", path.display());
                Ok(Some(graph))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse index at {}: {:?}. Will rebuild.",
                    path.display(),
                    e
                );
                let _ = std::fs::remove_file(path);
                Ok(None)
            }
        }
    }

    fn save_to_disk(
        graph: &CodeGraph,
        path: &Path,
        lang_caps: Arc<Vec<LanguageCaps>>,
        build_caps: Arc<Vec<BuildCaps>>,
    ) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let get_codec = |lang: &str| -> Option<Arc<dyn crate::bridge::NodeMetadataCodec>> {
            for caps in lang_caps.iter() {
                if caps.language.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            for caps in build_caps.iter() {
                if caps.build_tool.as_str() == lang {
                    return caps.metadata_codec.metadata_codec();
                }
            }
            None
        };

        // Serialize the graph
        let bytes = graph.serialize(get_codec)?;

        // Write to file atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(temp_path, path)?;

        tracing::info!("Saved index to {}", path.display());

        Ok(())
    }

    fn build_index(
        project_root: &Path,
        build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_tx: tokio::sync::mpsc::UnboundedSender<StubRequest>,
        global_routes: HashMap<String, Vec<PathBuf>>,
    ) -> Result<(CodeGraph, Vec<StubRequest>)> {
        // Scan and parse
        let parse_results =
            Scanner::scan_and_parse(project_root, &std::collections::HashMap::new());

        // Resolve
        let resolver = IndexResolver::with_caps((*build_caps).clone(), (*lang_caps).clone())
            .with_stubbing(StubbingManager::new(stub_tx));

        // resolve() now returns both ops and the filled ProjectContext
        let (ops, _project_context) = resolver.resolve(parse_results)?;

        // Build graph
        let mut builder = CodeGraphBuilder::new();

        // Register naming conventions
        for caps in lang_caps.iter() {
            if let Some(nc) = caps.presentation.naming_convention() {
                builder.naming_conventions.insert(caps.language.clone(), nc);
            }
        }

        builder.apply_ops(ops.clone())?;

        let stubs = resolver.resolve_stubs(&ops, &global_routes);

        Ok((builder.build(), stubs))
    }

    pub fn get_stub_cache(&self) -> Arc<crate::cache::GlobalStubCache> {
        self.stub_cache.clone()
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
