//! Core indexing engine with MVCC support

use crate::error::{NaviscopeError, Result};
use crate::ingest::builder::CodeGraphBuilder;
use crate::ingest::resolver::engine::IndexResolver;
use crate::ingest::scanner::Scanner;
use crate::model::CodeGraph;
use crate::model::GraphOp;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;
use xxhash_rust::xxh3::xxh3_64;
use std::collections::HashMap;
use naviscope_plugin::NamingConvention;

use crate::plugin::{BuildToolPlugin, LanguagePlugin};

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

    /// Plugins
    build_plugins: Arc<Vec<Arc<dyn BuildToolPlugin>>>,
    lang_plugins: Arc<Vec<Arc<dyn LanguagePlugin>>>,

    /// Runtime registry: language name -> naming convention
    naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,

    /// Cancellation token for background tasks (like watcher)
    cancel_token: tokio_util::sync::CancellationToken,
}

impl Drop for NaviscopeEngine {
    fn drop(&mut self) {
        self.cancel_token.cancel();
    }
}

impl NaviscopeEngine {
    /// Create a new engine
    pub fn new(project_root: PathBuf) -> Self {
        let canonical_root = project_root
            .canonicalize()
            .unwrap_or_else(|_| project_root.clone());
        let index_path = Self::compute_index_path(&canonical_root);

        Self {
            current: Arc::new(RwLock::new(Arc::new(CodeGraph::empty()))),
            project_root: canonical_root,
            index_path,
            build_plugins: Arc::new(Vec::new()),
            lang_plugins: Arc::new(Vec::new()),
            naming_conventions: Arc::new(std::collections::HashMap::new()),
            cancel_token: tokio_util::sync::CancellationToken::new(),
        }
    }

    pub fn register_language(&mut self, plugin: Arc<dyn LanguagePlugin>) {
        // Register naming convention if available
        if let Some(nc) = plugin.get_naming_convention() {
            let mut conventions = (*self.naming_conventions).clone();
            conventions.insert(plugin.name().to_string(), nc);
            self.naming_conventions = Arc::new(conventions);
        }

        let mut plugins = (*self.lang_plugins).clone();
        plugins.push(plugin);
        self.lang_plugins = Arc::new(plugins);
    }

    pub fn register_build_tool(&mut self, plugin: Arc<dyn BuildToolPlugin>) {
        // Register naming convention if available
        if let Some(nc) = plugin.get_naming_convention() {
            let mut conventions = (*self.naming_conventions).clone();
            conventions.insert(plugin.name().to_string(), nc);
            self.naming_conventions = Arc::new(conventions);
        }

        let mut plugins = (*self.build_plugins).clone();
        plugins.push(plugin);
        self.build_plugins = Arc::new(plugins);
    }

    /// Get the project root path
    pub fn root_path(&self) -> &Path {
        &self.project_root
    }

    /// Get the index resolver configured with current plugins
    pub fn get_resolver(&self) -> IndexResolver {
        IndexResolver::with_plugins((*self.build_plugins).clone(), (*self.lang_plugins).clone())
    }

    /// Get naming conventions registry (cheap Arc clone)
    pub(crate) fn naming_conventions(&self) -> Arc<std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>> {
        self.naming_conventions.clone()
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
        let lang_plugins = self.lang_plugins.clone();
        let build_plugins = self.build_plugins.clone();

        // Load in blocking pool
        let graph_opt = tokio::task::spawn_blocking(move || {
            Self::load_from_disk(&path, lang_plugins, build_plugins)
        })
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
        let lang_plugins = self.lang_plugins.clone();
        let build_plugins = self.build_plugins.clone();

        tokio::task::spawn_blocking(move || {
            Self::save_to_disk(&graph, &path, lang_plugins, build_plugins)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?
    }

    /// Rebuild the index from scratch
    pub async fn rebuild(&self) -> Result<()> {
        let project_root = self.project_root.clone();
        let build_plugins = self.build_plugins.clone();
        let lang_plugins = self.lang_plugins.clone();

        // Build in blocking pool (CPU-intensive)
        let new_graph = tokio::task::spawn_blocking(move || {
            Self::build_index(&project_root, build_plugins, lang_plugins)
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        // Atomically update (write lock held for microseconds)
        {
            let mut lock = self.current.write().await;
            *lock = Arc::new(new_graph);
        }

        // Save to disk
        self.save().await?;

        Ok(())
    }

    /// Update specific files incrementally
    pub async fn update_files(&self, files: Vec<PathBuf>) -> Result<()> {
        let base_graph = self.snapshot().await;
        let build_plugins = self.build_plugins.clone();
        let lang_plugins = self.lang_plugins.clone();

        // Prepare existing file metadata for change detection
        let mut existing_metadata = std::collections::HashMap::new();
        for (path, entry) in base_graph.file_index() {
            existing_metadata.insert(
                PathBuf::from(base_graph.symbols().resolve(&path.0)),
                entry.metadata.clone(),
            );
        }

        let current_lock = self.current.clone();

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

            let resolver =
                IndexResolver::with_plugins((*build_plugins).clone(), (*lang_plugins).clone());

            // 2. Phase 1: Heavy Build Resolution (Global Context)
            let mut project_context = crate::ingest::resolver::ProjectContext::new();
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
            let build_ops = resolver.resolve_build_batch(&build_files, &mut project_context)?;
            initial_ops.extend(build_ops);

            // 3. Phase 2: Pipeline Batch Processing for source files
            let pipeline = crate::ingest::pipeline::IngestPipeline::new(500); // 500 files per batch
            let source_paths: Vec<PathBuf> = source_files
                .into_iter()
                .map(|f| f.path().to_path_buf())
                .collect();

            let mut builder = base_graph.to_builder();

            // Register naming conventions
            for plugin in lang_plugins.iter() {
                if let Some(nc) = plugin.get_naming_convention() {
                    builder.naming_conventions.insert(plugin.name(), nc);
                }
            }

            builder.apply_ops(initial_ops)?;

            // Note: We are in a blocking thread, resolver and context are Thread-safe.
            pipeline.execute(&project_context, source_paths, &resolver, |batch_ops| {
                builder.apply_ops(batch_ops)?;
                Ok(())
            })?;

            // 4. Final Swap
            let final_graph = Arc::new(builder.build());
            let rt = tokio::runtime::Handle::current();
            rt.block_on(async {
                let mut lock = current_lock.write().await;
                *lock = final_graph;
            });

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

    /// Watch for filesystem changes and update incrementally
    pub async fn watch(self: Arc<Self>) -> Result<()> {
        use crate::runtime::watcher::Watcher;
        use std::collections::HashSet;
        use std::time::Duration;

        let root = self.project_root.clone();
        let mut watcher =
            Watcher::new(&root).map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        let engine_weak = Arc::downgrade(&self);
        let cancel_token = self.cancel_token.clone();

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
        lang_plugins: Arc<Vec<Arc<dyn LanguagePlugin>>>,
        build_plugins: Arc<Vec<Arc<dyn BuildToolPlugin>>>,
    ) -> Result<Option<CodeGraph>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(path)?;

        let get_plugin = |lang: &str| -> Option<Arc<dyn crate::plugin::NodeAdapter>> {
            for p in lang_plugins.iter() {
                if p.name().as_str() == lang {
                    return p.get_node_adapter();
                }
            }
            for p in build_plugins.iter() {
                if p.name().as_str() == lang {
                    return p.get_node_adapter();
                }
            }
            None
        };

        match CodeGraph::deserialize(&bytes, get_plugin) {
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
        lang_plugins: Arc<Vec<Arc<dyn LanguagePlugin>>>,
        build_plugins: Arc<Vec<Arc<dyn BuildToolPlugin>>>,
    ) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let get_plugin = |lang: &str| -> Option<Arc<dyn crate::plugin::NodeAdapter>> {
            for p in lang_plugins.iter() {
                if p.name().as_str() == lang {
                    return p.get_node_adapter();
                }
            }
            for p in build_plugins.iter() {
                if p.name().as_str() == lang {
                    return p.get_node_adapter();
                }
            }
            None
        };

        // Serialize the graph
        let bytes = graph.serialize(get_plugin)?;

        // Write to file atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(temp_path, path)?;

        tracing::info!("Saved index to {}", path.display());

        Ok(())
    }

    fn build_index(
        project_root: &Path,
        build_plugins: Arc<Vec<Arc<dyn BuildToolPlugin>>>,
        lang_plugins: Arc<Vec<Arc<dyn LanguagePlugin>>>,
    ) -> Result<CodeGraph> {
        // Scan and parse
        let parse_results =
            Scanner::scan_and_parse(project_root, &std::collections::HashMap::new());

        // Resolve
        let resolver =
            IndexResolver::with_plugins((*build_plugins).clone(), (*lang_plugins).clone());
        let ops = resolver.resolve(parse_results)?;

        // Build graph
        let mut builder = CodeGraphBuilder::new();

        // Register naming conventions
        for plugin in lang_plugins.iter() {
            if let Some(nc) = plugin.get_naming_convention() {
                builder.naming_conventions.insert(plugin.name(), nc);
            }
        }

        builder.apply_ops(ops)?;

        Ok(builder.build())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_snapshot_is_fast() {
        let engine = NaviscopeEngine::new(PathBuf::from("."));

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

        let engine = Arc::new(NaviscopeEngine::new(PathBuf::from(".")));

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
