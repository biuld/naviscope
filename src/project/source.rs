use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Build tool types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BuildTool {
    Gradle,
    Maven,
    Cargo,
    Npm,
    Poetry,
    Bazel,
}

/// Programming language types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Java,
    Kotlin,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    BuildFile, // For build files themselves
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceFile {
    pub path: PathBuf,
    pub content_hash: u64,
    pub last_modified: u64, // UNIX timestamp
}

impl SourceFile {
    pub fn new(
        path: PathBuf,
        content_hash: u64,
        last_modified: u64,
    ) -> Self {
        Self {
            path,
            content_hash,
            last_modified,
        }
    }
}
