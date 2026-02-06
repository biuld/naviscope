use crate::interner::SymbolInterner;
use naviscope_api::models::graph::{
    DisplaySymbolLocation, EdgeType, EmptyMetadata, NodeKind, NodeMetadata, NodeSource,
    ResolutionStatus,
};
use naviscope_api::models::symbol::{NodeId, Range};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::OnceLock;
use tree_sitter::Tree;

/// A function type that can deserialize bytes into language-specific IndexMetadata.
pub type MetadataDeserializer = fn(version: u32, bytes: &[u8]) -> Arc<dyn IndexMetadata>;

/// Global registry for metadata deserializers.
static DESERIALIZER_REGISTRY: OnceLock<std::sync::RwLock<HashMap<String, MetadataDeserializer>>> =
    OnceLock::new();

/// Register a deserializer for a specific metadata type tag.
pub fn register_metadata_deserializer(type_tag: &str, deserializer: MetadataDeserializer) {
    let registry = DESERIALIZER_REGISTRY.get_or_init(|| std::sync::RwLock::new(HashMap::new()));
    let mut registry = registry
        .write()
        .expect("Failed to lock deserializer registry");
    registry.insert(type_tag.to_string(), deserializer);
}

/// Serialized metadata for cache storage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedMetadata {
    /// Type tag identifying the metadata format (e.g., "java")
    pub type_tag: String,

    /// Schema version for the metadata format
    pub version: u32,

    /// The serialized metadata bytes
    #[serde(with = "serde_bytes")]
    pub data: Vec<u8>,
}

impl CachedMetadata {
    /// Create a new empty metadata
    pub fn empty() -> Self {
        Self {
            type_tag: "empty".to_string(),
            version: 0,
            data: Vec::new(),
        }
    }
}

/// Deserialize metadata from CachedMetadata using the registered deserializers.
pub fn deserialize_metadata(cached: &CachedMetadata) -> Arc<dyn IndexMetadata> {
    if cached.data.is_empty() || cached.type_tag == "empty" {
        return Arc::new(EmptyMetadata);
    }

    if let Some(registry) = DESERIALIZER_REGISTRY.get() {
        let registry = registry
            .read()
            .expect("Failed to read deserializer registry");
        if let Some(deserializer) = registry.get(&cached.type_tag) {
            return deserializer(cached.version, &cached.data);
        }
    }

    // Fallback: If "java" is not registered but type is "java", return empty
    // but try to warn if appropriate (though we want to avoid log spam)
    Arc::new(EmptyMetadata)
}

/// Compilation-time/Index-time metadata.
/// This version usually contains strings and is used during the parsing phase.
/// It must be able to convert itself into a runtime NodeMetadata.
pub trait IndexMetadata: Send + Sync + std::fmt::Debug {
    /// Cast to Any for downcasting to concrete types.
    fn as_any(&self) -> &dyn std::any::Any;

    /// Transform this metadata into its interned/optimized version for graph storage.
    fn intern(&self, interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata>;

    /// Convert this metadata into a cacheable form.
    /// Default implementation returns empty metadata.
    fn to_cached_metadata(&self) -> CachedMetadata {
        CachedMetadata::empty()
    }
}

impl IndexMetadata for EmptyMetadata {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn intern(&self, _interner: &mut dyn SymbolInterner) -> Arc<dyn NodeMetadata> {
        Arc::new(self.clone())
    }
}

/// Node model during the parsing phase, before interning
/// It holds raw Strings and strongly-typed Metadata
#[derive(Debug, Clone)]
pub struct IndexNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub lang: String,
    pub source: NodeSource,
    pub status: ResolutionStatus,
    pub location: Option<DisplaySymbolLocation>,
    pub metadata: Arc<dyn IndexMetadata>,
}

/// Relation model during the parsing phase
#[derive(Debug, Clone)]
pub struct IndexRelation {
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub edge_type: EdgeType,
    pub range: Option<Range>,
}

/// Core model produced by the parser
#[derive(Debug, Clone, Default)]
pub struct ParseOutput {
    pub nodes: Vec<IndexNode>,
    pub relations: Vec<IndexRelation>,
    /// All identifiers appearing in the file (used for global search and reference indexing)
    pub identifiers: Vec<String>,
}

/// Result of a global file parsing for indexing.
#[derive(Clone)]
pub struct GlobalParseResult {
    pub package_name: Option<String>,
    pub imports: Vec<String>,
    pub output: ParseOutput,
    pub source: Option<String>,
    pub tree: Option<Tree>,
}

/// Result of parsing a build file
pub struct BuildParseResult {
    // For now, mirroring what we have. Can be expanded.
    pub content: BuildContent,
}

#[derive(Debug, Clone)]
pub enum BuildContent {
    Parsed(ParseOutput),
    Metadata(serde_json::Value),
    Unparsed(String),
}

#[derive(Clone)]
pub enum ParsedContent {
    Language(GlobalParseResult),
    Metadata(serde_json::Value),
    Unparsed(String),
    /// Content not yet loaded into memory
    Lazy,
}

#[derive(Clone)]
pub struct ParsedFile {
    pub file: SourceFile,
    pub content: ParsedContent,
}

impl ParsedFile {
    pub fn path(&self) -> &Path {
        &self.file.path
    }

    pub fn is_build(&self) -> bool {
        match self.content {
            ParsedContent::Unparsed(..) | ParsedContent::Lazy => {
                let name = self
                    .path()
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("");
                name == "build.gradle"
                    || name == "build.gradle.kts"
                    || name == "settings.gradle"
                    || name == "settings.gradle.kts"
                    || name == "pom.xml"
            }
            _ => false,
        }
    }

    pub fn build_tool(&self) -> Option<naviscope_api::models::BuildTool> {
        if self.is_build() {
            let name = self
                .path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if name.ends_with(".gradle") || name.ends_with(".gradle.kts") {
                Some(naviscope_api::models::BuildTool::GRADLE)
            } else if name == "pom.xml" {
                Some(naviscope_api::models::BuildTool::MAVEN)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn language(&self) -> Option<naviscope_api::models::Language> {
        match self.content {
            ParsedContent::Language(ref res) => {
                // Try to infer from package (very basic heuristic)
                if let Some(ref pkg) = res.package_name {
                    if pkg.starts_with("java.") || pkg.starts_with("javax.") {
                        return Some(naviscope_api::models::Language::JAVA);
                    }
                }
                Some(naviscope_api::models::Language::UNKNOWN)
            }
            ParsedContent::Metadata(..) => None,
            ParsedContent::Unparsed(..) | ParsedContent::Lazy => {
                if self.is_build() {
                    self.build_tool()
                        .map(|t| naviscope_api::models::Language::new(t.as_str().to_string()))
                } else {
                    self.path()
                        .extension()
                        .and_then(|e| e.to_str())
                        .and_then(naviscope_api::models::Language::from_extension)
                }
            }
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
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
