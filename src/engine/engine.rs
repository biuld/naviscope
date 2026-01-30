//! Core indexing engine with MVCC support

use super::{CodeGraph, CodeGraphBuilder};
use crate::error::{NaviscopeError, Result};
use crate::project::scanner::Scanner;
use crate::resolver::engine::IndexResolver;
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
}

impl NaviscopeEngine {
    /// Create a new engine
    pub fn new(project_root: PathBuf) -> Self {
        let index_path = Self::compute_index_path(&project_root);

        Self {
            current: Arc::new(RwLock::new(Arc::new(CodeGraph::empty()))),
            project_root,
            index_path,
        }
    }

    /// Get the project root path
    pub fn root_path(&self) -> &Path {
        &self.project_root
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
        CodeGraph::clone(&*lock) // Arc clone, O(1)
    }

    /// Load index from disk
    pub async fn load(&self) -> Result<bool> {
        let path = self.index_path.clone();

        // Load in blocking pool
        let graph_opt = tokio::task::spawn_blocking(move || Self::load_from_disk(&path))
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

        tokio::task::spawn_blocking(move || Self::save_to_disk(&graph, &path))
            .await
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?
    }

    /// Rebuild the index from scratch
    pub async fn rebuild(&self) -> Result<()> {
        let project_root = self.project_root.clone();

        // Build in blocking pool (CPU-intensive)
        let new_graph = tokio::task::spawn_blocking(move || Self::build_index(&project_root))
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
    pub async fn update_files(&self, _files: Vec<PathBuf>) -> Result<()> {
        // For now, just rebuild everything
        // TODO: implement true incremental updates
        self.rebuild().await
    }

    /// Refresh index (detect changes and update)
    pub async fn refresh(&self) -> Result<()> {
        // For now, just rebuild
        // TODO: implement change detection
        self.rebuild().await
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

    fn load_from_disk(path: &Path) -> Result<Option<CodeGraph>> {
        if !path.exists() {
            return Ok(None);
        }

        let bytes = std::fs::read(path)?;

        match CodeGraph::deserialize(&bytes) {
            Ok(graph) => {
                tracing::info!("Loaded index from {}", path.display());
                Ok(Some(graph))
            }
            Err(e) => {
                tracing::warn!(
                    "Failed to parse index at {}: {}. Will rebuild.",
                    path.display(),
                    e
                );
                let _ = std::fs::remove_file(path);
                Ok(None)
            }
        }
    }

    fn save_to_disk(graph: &CodeGraph, path: &Path) -> Result<()> {
        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        // Serialize the graph
        let bytes = graph
            .serialize()
            .map_err(|e| NaviscopeError::Internal(format!("Serialization failed: {}", e)))?;

        // Write to file atomically (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        std::fs::write(&temp_path, bytes)?;
        std::fs::rename(temp_path, path)?;

        tracing::info!("Saved index to {}", path.display());

        Ok(())
    }

    fn build_index(project_root: &Path) -> Result<CodeGraph> {
        // Scan and parse
        let parse_results =
            Scanner::scan_and_parse(project_root, &std::collections::HashMap::new());

        // Resolve
        let resolver = IndexResolver::new();
        let ops = resolver.resolve(parse_results)?;

        // Build graph
        let mut builder = CodeGraphBuilder::new();
        builder.apply_ops(ops)?;

        // Update file map
        // TODO: Add file metadata

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
