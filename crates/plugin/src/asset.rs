//! Asset and Stub trait definitions for the Global Asset Scanner architecture.
//!
//! This module defines the core abstractions for:
//! - Asset discovery (finding JAR files, JDK modules, etc.)
//! - Asset indexing (extracting package prefixes from assets)
//! - Stub generation (creating type stubs from bytecode)
//! - Route registry (mapping FQNs to asset paths)

use crate::model::IndexNode;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Error type for asset operations
pub type BoxError = Box<dyn std::error::Error + Send + Sync>;

// ==================== Asset Source ====================

/// Asset source type - where the asset comes from
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AssetSource {
    /// JDK standard library (lib/modules, rt.jar, jmods)
    Jdk {
        version: Option<String>, // e.g. "17.0.1"
        path: PathBuf,           // JDK root path
    },

    /// Gradle cache
    Gradle {
        group: String,    // e.g. "io.netty"
        artifact: String, // e.g. "netty-common"
        version: String,  // e.g. "4.1.100.Final"
    },

    /// Maven local repository
    Maven {
        group: String,
        artifact: String,
        version: String,
    },

    /// Project local dependency (lib/*.jar)
    Local { project_path: PathBuf },

    /// Unknown source
    Unknown,
}

impl AssetSource {
    /// Get the source type as a string (for statistics and filtering)
    pub fn source_type(&self) -> &'static str {
        match self {
            AssetSource::Jdk { .. } => "jdk",
            AssetSource::Gradle { .. } => "gradle",
            AssetSource::Maven { .. } => "maven",
            AssetSource::Local { .. } => "local",
            AssetSource::Unknown => "unknown",
        }
    }
}

/// Asset entry with source metadata
#[derive(Debug, Clone)]
pub struct AssetEntry {
    pub path: PathBuf,
    pub source: AssetSource,
}

impl AssetEntry {
    pub fn new(path: PathBuf, source: AssetSource) -> Self {
        Self { path, source }
    }

    pub fn unknown(path: PathBuf) -> Self {
        Self {
            path,
            source: AssetSource::Unknown,
        }
    }
}

// ==================== Asset Layer ====================

/// Asset discoverer - knows where to find assets
/// Uses Iterator pattern for streaming (constant memory)
pub trait AssetDiscoverer: Send + Sync {
    /// Returns an asset iterator (streaming, does not load all at once)
    fn discover(&self) -> Box<dyn Iterator<Item = AssetEntry> + Send + '_>;

    /// Discoverer name (for logging/debugging)
    fn name(&self) -> &str;

    /// Default source type for this discoverer
    fn source_type(&self) -> &str;
}

/// Asset indexer - knows how to read asset internal structure
/// Returns package prefixes found in the asset
pub trait AssetIndexer: Send + Sync {
    /// Check if this indexer can handle the asset
    fn can_index(&self, asset: &Path) -> bool;

    /// Extract package prefixes from the asset
    /// Returns a list of package prefixes (e.g., "java.lang", "io.netty.channel")
    fn index(&self, asset: &Path) -> Result<Vec<String>, BoxError>;
}

/// Asset route registry - manages prefix → AssetEntry[] mapping
pub trait AssetRouteRegistry: Send + Sync {
    /// Register a route (with source info)
    fn register(&self, prefix: String, entry: AssetEntry);

    /// Query asset entries for an FQN (with source info)
    fn lookup(&self, fqn: &str) -> Option<Vec<AssetEntry>>;

    /// Query by source type
    fn lookup_by_source(&self, fqn: &str, source_type: &str) -> Option<Vec<AssetEntry>>;

    /// Get all routes (for serialization)
    fn all_routes(&self) -> HashMap<String, Vec<AssetEntry>>;

    /// Get statistics
    fn stats(&self) -> RegistryStats;
}

/// Asset source locator - maps a binary asset to its source asset if available
pub trait AssetSourceLocator: Send + Sync {
    fn locate_source(&self, entry: &AssetEntry) -> Option<PathBuf>;
}

/// Registry statistics
#[derive(Debug, Default, Clone)]
pub struct RegistryStats {
    pub total_prefixes: usize,
    pub total_entries: usize,
    pub by_source: HashMap<String, usize>, // e.g. {"jdk": 100, "gradle": 5000}
}

// ==================== Stub Layer ====================

/// Stub generator - knows how to generate type info from asset
/// Implemented by language plugins (e.g. JavaExternalResolver)
pub trait StubGenerator: Send + Sync {
    /// Check if this generator can handle the asset
    fn can_generate(&self, asset: &Path) -> bool;

    /// Generate stub for the specified FQN from asset
    /// source is used to set the source info on generated nodes
    fn generate(&self, fqn: &str, entry: &AssetEntry) -> Result<IndexNode, BoxError>;
}

/// Stub request (with source info)
#[derive(Debug, Clone)]
pub struct StubRequest {
    pub fqn: String,
    pub candidate_entries: Vec<AssetEntry>, // candidates with source info
}

impl StubRequest {
    pub fn new(fqn: String, candidate_entries: Vec<AssetEntry>) -> Self {
        Self {
            fqn,
            candidate_entries,
        }
    }

    /// Create from legacy format (paths only)
    pub fn from_paths(fqn: String, paths: Vec<PathBuf>) -> Self {
        Self {
            fqn,
            candidate_entries: paths.into_iter().map(AssetEntry::unknown).collect(),
        }
    }
}

/// Stub request sender (producer side)
pub trait StubRequestSender: Send + Sync {
    fn send(&self, request: StubRequest);
}

/// Stub request receiver (consumer side)
pub trait StubRequestReceiver: Send {
    fn recv(&mut self) -> Option<StubRequest>;
}
