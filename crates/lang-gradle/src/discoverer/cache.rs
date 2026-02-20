//! Gradle cache asset discoverer.
//!
//! Discovers JAR files from the Gradle cache directory:
//! `~/.gradle/caches/modules-2/files-2.1`

use naviscope_plugin::{AssetDiscoverer, AssetEntry, AssetSource};
use std::path::PathBuf;
use walkdir::WalkDir;

/// Gradle cache asset discoverer
pub struct GradleCacheDiscoverer {
    cache_path: Option<PathBuf>,
}

impl GradleCacheDiscoverer {
    pub fn new() -> Self {
        let cache_path = dirs::home_dir().map(|h| h.join(".gradle/caches/modules-2/files-2.1"));

        Self { cache_path }
    }

    /// Create with a custom cache path (for testing)
    pub fn with_path(path: PathBuf) -> Self {
        Self {
            cache_path: Some(path),
        }
    }

    /// Parse Gradle cache path to extract Maven coordinates
    /// Path format: ~/.gradle/caches/modules-2/files-2.1/{group}/{artifact}/{version}/{hash}/{file}
    fn parse_cache_path(&self, path: &std::path::Path) -> AssetSource {
        let Some(cache_root) = &self.cache_path else {
            return AssetSource::Unknown;
        };

        let Ok(relative) = path.strip_prefix(cache_root) else {
            return AssetSource::Unknown;
        };

        let components: Vec<_> = relative.components().collect();

        // Expected: group/artifact/version/hash/file.jar
        if components.len() >= 4 {
            let group = components[0].as_os_str().to_string_lossy().to_string();
            let artifact = components[1].as_os_str().to_string_lossy().to_string();
            let version = components[2].as_os_str().to_string_lossy().to_string();

            return AssetSource::Gradle {
                group,
                artifact,
                version,
            };
        }

        AssetSource::Unknown
    }
}

impl Default for GradleCacheDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetDiscoverer for GradleCacheDiscoverer {
    fn discover(&self) -> Box<dyn Iterator<Item = AssetEntry> + Send + '_> {
        let Some(cache_path) = &self.cache_path else {
            return Box::new(std::iter::empty());
        };

        if !cache_path.exists() {
            return Box::new(std::iter::empty());
        }

        // Use WalkDir for lazy/streaming directory traversal
        let cache_path_clone = cache_path.clone();
        Box::new(
            WalkDir::new(&cache_path_clone)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    // Only include JAR files
                    path.extension().and_then(|ext| ext.to_str()) == Some("jar")
                })
                .filter(|e| {
                    // Exclude sources and javadoc JARs
                    let name = e.path().file_name().and_then(|n| n.to_str()).unwrap_or("");
                    !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar")
                })
                .map(move |e| {
                    let path = e.path().to_path_buf();
                    let source = self.parse_cache_path(&path);
                    AssetEntry::new(path, source)
                }),
        )
    }

    fn name(&self) -> &str {
        "Gradle Cache Discoverer"
    }

    fn source_type(&self) -> &str {
        "gradle"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_gradle_cache_discoverer_empty() {
        let temp = tempfile::tempdir().unwrap();
        let discoverer = GradleCacheDiscoverer::with_path(temp.path().to_path_buf());

        let assets: Vec<_> = discoverer.discover().collect();
        assert!(assets.is_empty());
    }

    #[test]
    fn test_gradle_cache_discoverer_finds_jars() {
        let temp = tempfile::tempdir().unwrap();
        let cache_path = temp.path().to_path_buf();

        // Create mock Gradle cache structure
        // group/artifact/version/hash/file.jar
        let jar_dir = cache_path.join("io.netty/netty-common/4.1.100.Final/abc123");
        fs::create_dir_all(&jar_dir).unwrap();

        let jar_path = jar_dir.join("netty-common-4.1.100.Final.jar");
        fs::File::create(&jar_path).unwrap();

        // Create sources jar (should be excluded)
        let sources_jar = jar_dir.join("netty-common-4.1.100.Final-sources.jar");
        fs::File::create(&sources_jar).unwrap();

        let discoverer = GradleCacheDiscoverer::with_path(cache_path);
        let assets: Vec<_> = discoverer.discover().collect();

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0].path, jar_path);

        if let AssetSource::Gradle {
            group,
            artifact,
            version,
        } = &assets[0].source
        {
            assert_eq!(group, "io.netty");
            assert_eq!(artifact, "netty-common");
            assert_eq!(version, "4.1.100.Final");
        } else {
            panic!("Expected Gradle source");
        }
    }

    #[test]
    fn test_parse_cache_path() {
        let temp = tempfile::tempdir().unwrap();
        let cache_path = temp.path().to_path_buf();

        let discoverer = GradleCacheDiscoverer::with_path(cache_path.clone());

        let jar_path =
            cache_path.join("com.google.guava/guava/31.1-jre/deadbeef/guava-31.1-jre.jar");

        let source = discoverer.parse_cache_path(&jar_path);

        if let AssetSource::Gradle {
            group,
            artifact,
            version,
        } = source
        {
            assert_eq!(group, "com.google.guava");
            assert_eq!(artifact, "guava");
            assert_eq!(version, "31.1-jre");
        } else {
            panic!("Expected Gradle source");
        }
    }
}
