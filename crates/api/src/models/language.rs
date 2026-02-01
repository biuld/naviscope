use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::fmt;

/// Programming language types
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Language(Cow<'static, str>);

impl Language {
    pub const JAVA: Language = Language(Cow::Borrowed("java"));
    pub const KOTLIN: Language = Language(Cow::Borrowed("kotlin"));
    pub const RUST: Language = Language(Cow::Borrowed("rust"));
    pub const JAVASCRIPT: Language = Language(Cow::Borrowed("javascript"));
    pub const TYPESCRIPT: Language = Language(Cow::Borrowed("typescript"));
    pub const PYTHON: Language = Language(Cow::Borrowed("python"));
    pub const GO: Language = Language(Cow::Borrowed("go"));
    pub const BUILDFILE: Language = Language(Cow::Borrowed("buildfile"));
    pub const UNKNOWN: Language = Language(Cow::Borrowed("unknown"));

    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
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
            ext => Some(Self::new(ext.to_string())),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for Language {
    fn from(s: &str) -> Self {
        Self::new(s.to_string())
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
pub struct BuildTool(Cow<'static, str>);

impl BuildTool {
    pub const GRADLE: BuildTool = BuildTool(Cow::Borrowed("gradle"));
    pub const MAVEN: BuildTool = BuildTool(Cow::Borrowed("maven"));
    pub const CARGO: BuildTool = BuildTool(Cow::Borrowed("cargo"));
    pub const NPM: BuildTool = BuildTool(Cow::Borrowed("npm"));
    pub const POETRY: BuildTool = BuildTool(Cow::Borrowed("poetry"));
    pub const BAZEL: BuildTool = BuildTool(Cow::Borrowed("bazel"));

    pub fn new(name: impl Into<Cow<'static, str>>) -> Self {
        Self(name.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for BuildTool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for BuildTool {
    fn from(s: &str) -> Self {
        Self::new(s.to_string())
    }
}

impl AsRef<str> for BuildTool {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}
