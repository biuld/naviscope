use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use std::fmt;

/// Programming language types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Language(SmolStr);

impl Language {
    pub const JAVA: Language = Language(SmolStr::new_inline("java"));
    pub const KOTLIN: Language = Language(SmolStr::new_inline("kotlin"));
    pub const RUST: Language = Language(SmolStr::new_inline("rust"));
    pub const JAVASCRIPT: Language = Language(SmolStr::new_inline("javascript"));
    pub const TYPESCRIPT: Language = Language(SmolStr::new_inline("typescript"));
    pub const PYTHON: Language = Language(SmolStr::new_inline("python"));
    pub const GO: Language = Language(SmolStr::new_inline("go"));
    pub const BUILDFILE: Language = Language(SmolStr::new_inline("buildfile"));
    pub const UNKNOWN: Language = Language(SmolStr::new_inline("unknown"));

    pub fn new(name: impl Into<SmolStr>) -> Self {
        Self(name.into())
    }

    /// Map a file extension to a Language.
    /// This is the central logic for language detection from extensions.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_lowercase().as_str() {
            "java" => Some(Self::JAVA),
            "kt" | "kts" => Some(Self::KOTLIN),
            "rs" => Some(Self::RUST),
            "js" => Some(Self::JAVASCRIPT),
            "ts" => Some(Self::TYPESCRIPT),
            "py" => Some(Self::PYTHON),
            "go" => Some(Self::GO),
            "gradle" | "gradle.kts" => Some(Self::BUILDFILE),
            ext => Some(Self::new(ext)),
        }
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Language {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for Language {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Build tool types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct BuildTool(SmolStr);

impl BuildTool {
    pub const GRADLE: BuildTool = BuildTool(SmolStr::new_inline("gradle"));
    pub const MAVEN: BuildTool = BuildTool(SmolStr::new_inline("maven"));
    pub const CARGO: BuildTool = BuildTool(SmolStr::new_inline("cargo"));
    pub const NPM: BuildTool = BuildTool(SmolStr::new_inline("npm"));
    pub const POETRY: BuildTool = BuildTool(SmolStr::new_inline("poetry"));
    pub const BAZEL: BuildTool = BuildTool(SmolStr::new_inline("bazel"));

    pub fn new(name: impl Into<SmolStr>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for BuildTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for BuildTool {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl AsRef<str> for BuildTool {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
