pub mod scanner;
pub mod source;
pub mod watcher;

use std::path::Path;

/// Checks if a path is relevant to Naviscope (e.g., Java source or build files).
pub fn is_relevant_path(path: &Path) -> bool {
    path.extension()
        .and_then(|s| s.to_str())
        .map(|ext| ext == "java" || ext == "gradle" || ext == "kts")
        .unwrap_or(false)
}
