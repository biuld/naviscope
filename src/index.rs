use crate::error::{NaviscopeError, Result};
use crate::model::graph::{EdgeType, GraphEdge, GraphNode};
use crate::project::scanner::Scanner;
use crate::project::source::SourceFile;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing;
use xxhash_rust::xxh3::xxh3_64;

pub const CURRENT_VERSION: u32 = 1;
pub const DEFAULT_INDEX_DIR: &str = ".naviscope/indices";

#[derive(Serialize, Deserialize, Clone)]
pub struct CodeGraph {
    pub version: u32,
    pub topology: StableDiGraph<GraphNode, GraphEdge>,
    pub fqn_map: HashMap<String, NodeIndex>,
    pub name_map: HashMap<String, Vec<NodeIndex>>,
    pub file_map: HashMap<PathBuf, SourceFile>,
    pub path_to_nodes: HashMap<PathBuf, Vec<NodeIndex>>,
}

impl CodeGraph {
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            topology: StableDiGraph::new(),
            fqn_map: HashMap::new(),
            name_map: HashMap::new(),
            file_map: HashMap::new(),
            path_to_nodes: HashMap::new(),
        }
    }

    pub fn get_or_create_node(&mut self, id: &str, node_data: GraphNode) -> NodeIndex {
        if let Some(&idx) = self.fqn_map.get(id) {
            // Optional: Update node data if needed
            idx
        } else {
            let name = node_data.name().to_string();
            let idx = self.topology.add_node(node_data);
            self.fqn_map.insert(id.to_string(), idx);
            self.name_map.entry(name).or_default().push(idx);
            idx
        }
    }

    pub fn find_node_at(&self, path: &Path, line: usize, col: usize) -> Option<NodeIndex> {
        let nodes = self.path_to_nodes.get(path)?;

        for &idx in nodes {
            if let Some(node) = self.topology.node_weight(idx) {
                if let Some(range) = node.name_range() {
                    if range.contains(line, col) {
                        return Some(idx);
                    }
                }
            }
        }
        None
    }

    /// Finds an edge whose range contains the given position.
    /// This is used to find references from source code.
    pub fn find_edge_at(
        &self,
        path: &Path,
        line: usize,
        col: usize,
    ) -> Option<(NodeIndex, NodeIndex, &GraphEdge)> {
        let nodes = self.path_to_nodes.get(path)?;

        for &node_idx in nodes {
            // Check outgoing edges from nodes in this file
            let mut edges = self
                .topology
                .neighbors_directed(node_idx, petgraph::Direction::Outgoing)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = edges.next(&self.topology) {
                let edge = &self.topology[edge_idx];
                if let Some(range) = &edge.range {
                    if range.contains(line, col) {
                        return Some((node_idx, neighbor_idx, edge));
                    }
                }
            }
        }
        None
    }

    /// Finds nodes matching a symbol resolution result.
    /// This is a low-level query used by resolvers.
    pub fn find_matches_by_fqn(&self, fqn: &str) -> Vec<NodeIndex> {
        if let Some(&idx) = self.fqn_map.get(fqn) {
            vec![idx]
        } else {
            vec![]
        }
    }
}

#[derive(Clone)]
pub struct Naviscope {
    graph: CodeGraph,
    project_root: PathBuf,
}

impl Naviscope {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            graph: CodeGraph::new(),
            project_root,
        }
    }

    /// Gets the base directory for storing indices, supporting NAVISCOPE_INDEX_DIR env var.
    pub fn get_base_index_dir() -> PathBuf {
        if let Ok(env_dir) = std::env::var("NAVISCOPE_INDEX_DIR") {
            return PathBuf::from(env_dir);
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(DEFAULT_INDEX_DIR)
    }

    /// Clears all built indices by removing the base index directory.
    pub fn clear_all_indices() -> Result<()> {
        let base_dir = Self::get_base_index_dir();
        if base_dir.exists() {
            std::fs::remove_dir_all(&base_dir)?;
        }
        Ok(())
    }

    /// Clears the index for the current project.
    pub fn clear_project_index(&self) -> Result<()> {
        let path = self.get_project_index_path();
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Gets the index file path for the current project.
    fn get_project_index_path(&self) -> PathBuf {
        let base_dir = Self::get_base_index_dir();
        let abs_path = self
            .project_root
            .canonicalize()
            .unwrap_or(self.project_root.clone());
        let hash = xxh3_64(abs_path.to_string_lossy().as_bytes());
        base_dir.join(format!("{:016x}.bin", hash))
    }

    /// Loads the index for the project from the fixed storage path.
    /// Returns Ok(true) if loaded successfully, Ok(false) if file doesn't exist or is incompatible.
    /// Automatically handles incompatible binaries by cleaning up and resetting the graph.
    pub fn load(&mut self) -> Result<bool> {
        let path = self.get_project_index_path();
        if !path.exists() {
            return Ok(false);
        }

        let file = std::fs::File::open(&path)?;
        let reader = std::io::BufReader::new(file);
        match rmp_serde::from_read(reader) {
            Ok(graph) => {
                self.graph = graph;
                Ok(true)
            }
            Err(e) => {
                // Handle incompatible binary: log warning, clean up file, and reset graph
                tracing::warn!(
                    "Failed to parse index at {}: {}. Incompatible binary detected, will rebuild.",
                    path.display(),
                    e
                );
                self.graph = CodeGraph::new();
                // Try to remove the corrupted file (ignore errors)
                let _ = std::fs::remove_file(&path);
                Ok(false)
            }
        }
    }

    /// Saves the index to the fixed storage path.
    pub fn save(&self) -> Result<()> {
        let path = self.get_project_index_path();

        // Ensure directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        rmp_serde::encode::write(&mut writer, &self.graph)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }

    /// Saves the index to a file in JSON format for debugging.
    pub fn save_to_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.graph)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }

    pub fn build_index(&mut self) -> Result<()> {
        // Try to load existing index first if memory is empty
        if self.graph.file_map.is_empty() {
            let _ = self.load();
        }

        // Refresh will handle version compatibility check and rebuild if needed
        self.refresh()
    }

    /// Scans for changes and updates the graph in memory.
    /// Does not reload from disk, but saves to disk if changes are detected.
    /// Assumes the graph is already loaded in memory (via load() or previous refresh()).
    pub fn refresh(&mut self) -> Result<()> {
        use crate::model::graph::GraphOp;
        use crate::resolver::engine::IndexResolver;

        // Check version compatibility - if mismatch, clear disk index and reset graph
        if self.graph.version != CURRENT_VERSION {
            tracing::info!(
                "Index version mismatch (found {}, current {}). Rebuilding...",
                self.graph.version,
                CURRENT_VERSION
            );
            let _ = self.clear_project_index();
            // Reset to a fresh index with current version
            self.graph = CodeGraph::new();
        }

        // Phase 1: Scan and Parse (parallel I/O and CPU-intensive work)
        let parse_results = Scanner::scan_and_parse(&self.project_root, &self.graph.file_map);

        // Detect and handle deleted files
        let current_paths: HashSet<PathBuf> = Scanner::collect_paths(&self.project_root)
            .into_iter()
            .collect();

        let mut deleted_paths = Vec::new();
        for path in self.graph.file_map.keys() {
            if !current_paths.contains(path) {
                deleted_paths.push(path.clone());
            }
        }

        for path in &deleted_paths {
            self.apply_graph_op(GraphOp::RemovePath { path: path.clone() })?;
            self.graph.file_map.remove(path);
        }

        // Update file metadata for each parsed file
        for parsed in &parse_results {
            self.graph
                .file_map
                .insert(parsed.file.path.clone(), parsed.file.clone());
        }

        // Phase 2: Resolve (coordinated by Resolver in two phases)
        let resolver = IndexResolver::new();
        let all_ops = resolver.resolve(parse_results)?;

        // Phase 3: Apply (serial merge into the graph - fast memory operations)
        let has_changes = !all_ops.is_empty() || !deleted_paths.is_empty();
        for op in all_ops {
            self.apply_graph_op(op)?;
        }

        // Save updated index only if there were changes
        if has_changes {
            self.save()?;
        }

        Ok(())
    }

    /// Apply a single graph operation to the index
    fn apply_graph_op(&mut self, op: crate::model::graph::GraphOp) -> Result<()> {
        use crate::model::graph::GraphOp;

        match op {
            GraphOp::AddNode { id, data } => {
                let path = data.file_path().cloned();
                let idx = self.graph.get_or_create_node(&id, data);

                // Update path_to_nodes mapping
                if let Some(p) = path {
                    self.graph.path_to_nodes.entry(p).or_default().push(idx);
                }
            }
            GraphOp::AddEdge {
                from_id,
                to_id,
                edge,
            } => {
                // Look up node indices
                let from_idx = self.graph.fqn_map.get(&from_id).cloned();
                let to_idx = self.graph.fqn_map.get(&to_id).cloned();

                if let (Some(s_idx), Some(t_idx)) = (from_idx, to_idx) {
                    // For structural edges (Contains), avoid duplicates
                    if edge.edge_type == EdgeType::Contains {
                        let already_exists = self
                            .graph
                            .topology
                            .edges_connecting(s_idx, t_idx)
                            .any(|e| e.weight().edge_type == EdgeType::Contains);
                        if !already_exists {
                            self.graph.topology.add_edge(s_idx, t_idx, edge);
                        }
                    } else {
                        // For other edges (Calls, References, etc.), always add to capture multiple occurrences
                        self.graph.topology.add_edge(s_idx, t_idx, edge);
                    }
                }
            }
            GraphOp::RemovePath { path } => {
                if let Some(nodes) = self.graph.path_to_nodes.remove(&path) {
                    for node_idx in nodes {
                        // Get FQN before removing from graph
                        if let Some(node) = self.graph.topology.node_weight(node_idx) {
                            let fqn = node.fqn();
                            let name = node.name().to_string();
                            self.graph.fqn_map.remove(fqn);
                            if let Some(nodes_with_name) = self.graph.name_map.get_mut(&name) {
                                nodes_with_name.retain(|&idx| idx != node_idx);
                                if nodes_with_name.is_empty() {
                                    self.graph.name_map.remove(&name);
                                }
                            }
                        }
                        self.graph.topology.remove_node(node_idx);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }
}
