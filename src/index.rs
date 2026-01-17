use crate::error::{NaviscopeError, Result};
use crate::model::graph::{EdgeType, GraphNode};
use crate::project::scanner::Scanner;
use crate::project::source::SourceFile;
use petgraph::stable_graph::{NodeIndex, StableDiGraph};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use xxhash_rust::xxh3::xxh3_64;

pub const CURRENT_VERSION: u32 = 1;
pub const DEFAULT_INDEX_DIR: &str = ".naviscope/indices";

#[derive(Serialize, Deserialize)]
pub struct NaviscopeIndex {
    pub version: u32,
    pub graph: StableDiGraph<GraphNode, EdgeType>,
    pub fqn_map: HashMap<String, NodeIndex>,
    pub file_map: HashMap<PathBuf, SourceFile>,
    pub path_to_nodes: HashMap<PathBuf, Vec<NodeIndex>>,
}

impl NaviscopeIndex {
    pub fn new() -> Self {
        Self {
            version: CURRENT_VERSION,
            graph: StableDiGraph::new(),
            fqn_map: HashMap::new(),
            file_map: HashMap::new(),
            path_to_nodes: HashMap::new(),
        }
    }

    pub fn get_or_create_node(&mut self, id: &str, node_data: GraphNode) -> NodeIndex {
        if let Some(&idx) = self.fqn_map.get(id) {
            // Optional: Update node data if needed
            idx
        } else {
            let idx = self.graph.add_node(node_data);
            self.fqn_map.insert(id.to_string(), idx);
            idx
        }
    }
}

pub struct Naviscope {
    index: NaviscopeIndex,
    project_root: PathBuf,
}

impl Naviscope {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            index: NaviscopeIndex::new(),
            project_root,
        }
    }

    /// Gets the base directory for storing indices, supporting NAVISCOPE_INDEX_DIR env var.
    fn get_base_index_dir() -> PathBuf {
        if let Ok(env_dir) = std::env::var("NAVISCOPE_INDEX_DIR") {
            return PathBuf::from(env_dir);
        }

        let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
        Path::new(&home).join(DEFAULT_INDEX_DIR)
    }

    /// Gets the index file path for the current project.
    fn get_project_index_path(&self) -> PathBuf {
        let base_dir = Self::get_base_index_dir();
        let abs_path = self.project_root.canonicalize().unwrap_or(self.project_root.clone());
        let hash = xxh3_64(abs_path.to_string_lossy().as_bytes());
        base_dir.join(format!("{:016x}.bin", hash))
    }

    /// Loads the index for the project from the fixed storage path.
    pub fn load(&mut self) -> Result<()> {
        let path = self.get_project_index_path();
        if !path.exists() {
            return Ok(());
        }

        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let config = bincode::config::standard();
        self.index = bincode::serde::decode_from_std_read(&mut reader, config)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
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
        let config = bincode::config::standard();
        bincode::serde::encode_into_std_write(&self.index, &mut writer, config)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }

    /// Saves the index to a file in JSON format for debugging.
    pub fn save_to_json<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let writer = std::io::BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &self.index)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }

    /// Loads an index from a specific file path.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut reader = std::io::BufReader::new(file);
        let config = bincode::config::standard();
        let index: NaviscopeIndex = bincode::serde::decode_from_std_read(&mut reader, config)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(Self {
            index,
            project_root: PathBuf::new(), // Root is unknown when loading from arbitrary file
        })
    }

    /// Saves the index to a specific file path.
    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let file = std::fs::File::create(path)?;
        let mut writer = std::io::BufWriter::new(file);
        let config = bincode::config::standard();
        bincode::serde::encode_into_std_write(&self.index, &mut writer, config)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;
        Ok(())
    }

    pub fn build_index(&mut self) -> Result<()> {
        use crate::project::resolver::GraphOp;
        use crate::project::resolver::Resolver;

        // Try to load existing index first
        let _ = self.load();

        // Phase 1: Scan and Parse (parallel I/O and CPU-intensive work)
        let parse_results = Scanner::scan_and_parse(&self.project_root, &self.index.file_map);

        // Detect and handle deleted files
        let current_paths: HashSet<PathBuf> =
            Scanner::collect_paths(&self.project_root).into_iter().collect();

        let mut deleted_paths = Vec::new();
        for path in self.index.file_map.keys() {
            if !current_paths.contains(path) {
                deleted_paths.push(path.clone());
            }
        }

        for path in deleted_paths {
            self.apply_graph_op(GraphOp::RemovePath { path: path.clone() })?;
            self.index.file_map.remove(&path);
        }

        // Update file metadata for each parsed file
        for parsed in &parse_results {
            self.index
                .file_map
                .insert(parsed.file.path.clone(), parsed.file.clone());
        }

        // Phase 2: Resolve (coordinated by Resolver in two phases)
        let resolver = Resolver::new();
        let all_ops = resolver.resolve(parse_results)?;

        // Phase 3: Apply (serial merge into the graph - fast memory operations)
        for op in all_ops {
            self.apply_graph_op(op)?;
        }

        // Save updated index
        self.save()?;

        Ok(())
    }

    /// Apply a single graph operation to the index
    fn apply_graph_op(&mut self, op: crate::project::resolver::GraphOp) -> Result<()> {
        use crate::project::resolver::GraphOp;

        match op {
            GraphOp::AddNode { id, data } => {
                let path = data.file_path().cloned();
                let idx = self.index.get_or_create_node(&id, data);

                // Update path_to_nodes mapping
                if let Some(p) = path {
                    self.index
                        .path_to_nodes
                        .entry(p)
                        .or_default()
                        .push(idx);
                }
            }
            GraphOp::AddEdge {
                from_id,
                to_id,
                edge_type,
            } => {
                // Look up node indices
                let from_idx = self.index.fqn_map.get(&from_id).cloned();
                let to_idx = self.index.fqn_map.get(&to_id).cloned();

                if let (Some(s_idx), Some(t_idx)) = (from_idx, to_idx) {
                    // Only add edge if it doesn't exist
                    if !self.index.graph.contains_edge(s_idx, t_idx) {
                        self.index.graph.add_edge(s_idx, t_idx, edge_type);
                    }
                }
            }
            GraphOp::RemovePath { path } => {
                if let Some(nodes) = self.index.path_to_nodes.remove(&path) {
                    for node_idx in nodes {
                        // Get FQN before removing from graph
                        if let Some(node) = self.index.graph.node_weight(node_idx) {
                            let fqn = node.fqn();
                            self.index.fqn_map.remove(&fqn);
                        }
                        self.index.graph.remove_node(node_idx);
                    }
                }
            }
        }

        Ok(())
    }

    pub fn index(&self) -> &NaviscopeIndex {
        &self.index
    }
}
