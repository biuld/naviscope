use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Build tool types
// Re-export from API
pub use naviscope_api::models::{BuildTool, Language};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub content_hash: u64,
    pub last_modified: u64, // UNIX timestamp
}

impl SourceFile {
    pub fn new(path: PathBuf, content_hash: u64, last_modified: u64) -> Self {
        Self {
            path,
            content_hash,
            last_modified,
        }
    }
}
