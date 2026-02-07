//! In-memory implementation of AssetRouteRegistry.
//!
//! Provides thread-safe storage for FQN prefix → AssetEntry mappings.

use naviscope_plugin::{AssetEntry, AssetRouteRegistry, RegistryStats};
use std::collections::HashMap;
use std::sync::RwLock;

/// Thread-safe in-memory route registry
pub struct InMemoryRouteRegistry {
    /// Mapping from package prefix to asset entries
    routes: RwLock<HashMap<String, Vec<AssetEntry>>>,
}

impl InMemoryRouteRegistry {
    pub fn new() -> Self {
        Self {
            routes: RwLock::new(HashMap::new()),
        }
    }

    /// Create with initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            routes: RwLock::new(HashMap::with_capacity(capacity)),
        }
    }

    /// Register multiple routes at once (more efficient than individual calls)
    pub fn register_batch(&self, entries: impl IntoIterator<Item = (String, AssetEntry)>) {
        let mut routes = self.routes.write().unwrap();
        for (prefix, entry) in entries {
            routes.entry(prefix).or_default().push(entry);
        }
    }

    /// Clear all routes
    pub fn clear(&self) {
        let mut routes = self.routes.write().unwrap();
        routes.clear();
    }

    /// Get number of unique prefixes
    pub fn prefix_count(&self) -> usize {
        let routes = self.routes.read().unwrap();
        routes.len()
    }
}

impl Default for InMemoryRouteRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetRouteRegistry for InMemoryRouteRegistry {
    fn register(&self, prefix: String, entry: AssetEntry) {
        let mut routes = self.routes.write().unwrap();
        routes.entry(prefix).or_default().push(entry);
    }

    fn lookup(&self, fqn: &str) -> Option<Vec<AssetEntry>> {
        let routes = self.routes.read().unwrap();

        // Try exact match first
        if let Some(entries) = routes.get(fqn) {
            return Some(entries.clone());
        }

        // Try prefix matching (longest match wins)
        let mut best_match: Option<(&str, &Vec<AssetEntry>)> = None;

        for (prefix, entries) in routes.iter() {
            if fqn.starts_with(prefix) {
                // Check if this is a valid prefix (followed by '.' or end of string)
                let remainder = &fqn[prefix.len()..];
                if remainder.is_empty() || remainder.starts_with('.') {
                    match &best_match {
                        None => best_match = Some((prefix, entries)),
                        Some((best_prefix, _)) if prefix.len() > best_prefix.len() => {
                            best_match = Some((prefix, entries))
                        }
                        _ => {}
                    }
                }
            }
        }

        best_match.map(|(_, entries)| entries.clone())
    }

    fn lookup_by_source(&self, fqn: &str, source_type: &str) -> Option<Vec<AssetEntry>> {
        self.lookup(fqn).map(|entries| {
            entries
                .into_iter()
                .filter(|e| e.source.source_type() == source_type)
                .collect()
        })
    }

    fn all_routes(&self) -> HashMap<String, Vec<AssetEntry>> {
        let routes = self.routes.read().unwrap();
        routes.clone()
    }

    fn stats(&self) -> RegistryStats {
        let routes = self.routes.read().unwrap();

        let mut total_entries = 0;
        let mut by_source: HashMap<String, usize> = HashMap::new();

        for entries in routes.values() {
            total_entries += entries.len();
            for entry in entries {
                *by_source
                    .entry(entry.source.source_type().to_string())
                    .or_default() += 1;
            }
        }

        RegistryStats {
            total_prefixes: routes.len(),
            total_entries,
            by_source,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_plugin::AssetSource;
    use std::path::PathBuf;

    fn make_entry(path: &str, source: AssetSource) -> AssetEntry {
        AssetEntry::new(PathBuf::from(path), source)
    }

    #[test]
    fn test_register_and_lookup() {
        let registry = InMemoryRouteRegistry::new();

        let entry = make_entry(
            "/path/to/rt.jar",
            AssetSource::Jdk {
                version: Some("17".to_string()),
                path: PathBuf::from("/usr/lib/jvm/java-17"),
            },
        );

        registry.register("java.lang".to_string(), entry.clone());

        // Exact match
        let result = registry.lookup("java.lang");
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);

        // Prefix match
        let result = registry.lookup("java.lang.String");
        assert!(result.is_some());
        assert_eq!(result.unwrap().len(), 1);

        // No match
        let result = registry.lookup("com.example");
        assert!(result.is_none());

        // Invalid prefix (must be followed by '.' or end)
        registry.register(
            "java".to_string(),
            make_entry("/other.jar", AssetSource::Unknown),
        );
        let result = registry.lookup("javascript.foo");
        assert!(result.is_none());
    }

    #[test]
    fn test_longest_prefix_match() {
        let registry = InMemoryRouteRegistry::new();

        let entry1 = make_entry("/jdk.jar", AssetSource::Unknown);
        let entry2 = make_entry("/netty.jar", AssetSource::Unknown);

        registry.register("io".to_string(), entry1);
        registry.register("io.netty".to_string(), entry2);

        // Should match the longer prefix
        let result = registry.lookup("io.netty.channel.Channel");
        assert!(result.is_some());
        let entries = result.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, PathBuf::from("/netty.jar"));
    }

    #[test]
    fn test_lookup_by_source() {
        let registry = InMemoryRouteRegistry::new();

        let jdk_entry = make_entry(
            "/jdk.jar",
            AssetSource::Jdk {
                version: Some("17".to_string()),
                path: PathBuf::from("/jdk"),
            },
        );
        let gradle_entry = make_entry(
            "/gradle.jar",
            AssetSource::Gradle {
                group: "test".to_string(),
                artifact: "test".to_string(),
                version: "1.0".to_string(),
            },
        );

        registry.register("java.lang".to_string(), jdk_entry);
        registry.register("java.lang".to_string(), gradle_entry);

        // Filter by source
        let jdk_only = registry.lookup_by_source("java.lang", "jdk");
        assert!(jdk_only.is_some());
        assert_eq!(jdk_only.unwrap().len(), 1);

        let gradle_only = registry.lookup_by_source("java.lang", "gradle");
        assert!(gradle_only.is_some());
        assert_eq!(gradle_only.unwrap().len(), 1);
    }

    #[test]
    fn test_stats() {
        let registry = InMemoryRouteRegistry::new();

        registry.register(
            "java.lang".to_string(),
            make_entry(
                "/a.jar",
                AssetSource::Jdk {
                    version: None,
                    path: PathBuf::from("/jdk"),
                },
            ),
        );
        registry.register(
            "java.util".to_string(),
            make_entry(
                "/b.jar",
                AssetSource::Jdk {
                    version: None,
                    path: PathBuf::from("/jdk"),
                },
            ),
        );
        registry.register(
            "io.netty".to_string(),
            make_entry(
                "/c.jar",
                AssetSource::Gradle {
                    group: "io.netty".to_string(),
                    artifact: "netty".to_string(),
                    version: "4.1".to_string(),
                },
            ),
        );

        let stats = registry.stats();
        assert_eq!(stats.total_prefixes, 3);
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.by_source.get("jdk"), Some(&2));
        assert_eq!(stats.by_source.get("gradle"), Some(&1));
    }

    #[test]
    fn test_register_batch() {
        let registry = InMemoryRouteRegistry::new();

        let entries = vec![
            (
                "java.lang".to_string(),
                make_entry("/a.jar", AssetSource::Unknown),
            ),
            (
                "java.util".to_string(),
                make_entry("/b.jar", AssetSource::Unknown),
            ),
            (
                "java.io".to_string(),
                make_entry("/c.jar", AssetSource::Unknown),
            ),
        ];

        registry.register_batch(entries);

        assert_eq!(registry.prefix_count(), 3);
    }
}
