use serde::{Deserialize, Serialize};

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
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Java,
    Kotlin,
    Rust,
    JavaScript,
    TypeScript,
    Python,
    Go,
    BuildFile, // For build files themselves
    Other(String),
}
