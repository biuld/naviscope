pub mod builder;
pub mod parser;
pub mod pipeline;
pub mod resolver;
pub mod scanner;

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
