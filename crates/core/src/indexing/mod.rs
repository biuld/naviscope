pub mod compiler;
pub mod scanner;
pub(crate) mod source_runtime;

pub use naviscope_plugin::IndexNode;

/// A request to asynchronously generate a stub for an external FQN.
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub fqn: String,
    pub candidate_paths: Vec<std::path::PathBuf>,
}

use std::path::Path;

pub fn is_relevant_path(path: &Path) -> bool {
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        if name.starts_with('.') {
            return false;
        }
        if name == "target" || name == "build" || name == "node_modules" {
            return false;
        }
    }
    true
}
