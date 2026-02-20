use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Default)]
pub struct ProjectSymbolTable {
    pub type_symbols: std::collections::HashSet<String>,
    pub method_symbols: std::collections::HashSet<String>,
}

/// Project context generated during build indexing.
#[derive(Debug, Clone, Default)]
pub struct ProjectContext {
    /// Mapping from path prefixes to module IDs (e.g., "/project/app" -> "module::app")
    pub path_to_module: HashMap<PathBuf, String>,
    /// Project-level collected symbol snapshot used by analyze/bind stage.
    pub symbol_table: ProjectSymbolTable,
}

impl ProjectContext {
    pub fn new() -> Self {
        Self {
            path_to_module: HashMap::new(),
            symbol_table: ProjectSymbolTable::default(),
        }
    }

    /// Finds the best matching module ID for a given file path.
    pub fn find_module_for_path(&self, path: &Path) -> Option<String> {
        let mut current = path.to_path_buf();
        while let Some(parent) = current.parent() {
            if let Some(id) = self.path_to_module.get(parent) {
                return Some(id.clone());
            }
            current = parent.to_path_buf();
        }
        None
    }
}
