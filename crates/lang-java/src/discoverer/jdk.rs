//! JDK asset discoverer.
//!
//! Discovers JDK standard library assets from:
//! - JAVA_HOME environment variable
//! - macOS java_home tool
//! - Common installation paths
//! - SDKMAN

use naviscope_plugin::{AssetDiscoverer, AssetEntry, AssetSource};
use std::path::{Path, PathBuf};

/// JDK asset discoverer
pub struct JdkDiscoverer {
    /// Cached JDK assets (discovered once)
    cached_assets: Vec<AssetEntry>,
}

impl JdkDiscoverer {
    pub fn new() -> Self {
        let mut discoverer = Self {
            cached_assets: Vec::new(),
        };
        discoverer.discover_jdk();
        discoverer
    }

    /// Get the discovered JDK root path (if any)
    pub fn jdk_root(&self) -> Option<&Path> {
        self.cached_assets.first().map(|e| {
            // Navigate up from lib/modules or jre/lib/rt.jar to JDK root
            let path = &e.path;
            if path.ends_with("lib/modules") {
                path.parent().and_then(|p| p.parent())
            } else if path.ends_with("jre/lib/rt.jar") || path.ends_with("lib/rt.jar") {
                path.parent()
                    .and_then(|p| p.parent())
                    .and_then(|p| p.parent())
            } else {
                // jmods case
                path.parent().and_then(|p| p.parent())
            }
            .unwrap_or(path.as_path())
        })
    }

    fn discover_jdk(&mut self) {
        let mut jdk_root: Option<PathBuf> = None;

        // 1. Check JAVA_HOME
        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            let path = PathBuf::from(&java_home);
            if self.collect_sdk_assets(&path).is_some() {
                jdk_root = Some(path);
            }
        }

        // 2. macOS specific: Use java_home tool
        #[cfg(target_os = "macos")]
        if self.cached_assets.is_empty() {
            if let Ok(output) = std::process::Command::new("/usr/libexec/java_home").output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    let path = PathBuf::from(&path_str);
                    if self.collect_sdk_assets(&path).is_some() {
                        jdk_root = Some(path);
                    }
                }
            }
        }

        // 3. Search common installation paths
        if self.cached_assets.is_empty() {
            let mut search_roots = Vec::new();

            #[cfg(target_os = "macos")]
            {
                search_roots.push(PathBuf::from("/Library/Java/JavaVirtualMachines/"));
                search_roots.push(PathBuf::from("/opt/homebrew/opt/openjdk/"));
                search_roots.push(PathBuf::from("/usr/local/opt/openjdk/"));
            }
            #[cfg(target_os = "linux")]
            {
                search_roots.push(PathBuf::from("/usr/lib/jvm/"));
            }
            #[cfg(target_os = "windows")]
            {
                search_roots.push(PathBuf::from("C:\\Program Files\\Java\\"));
            }

            // SDKMAN
            if let Some(mut sdkman) = dirs::home_dir() {
                sdkman.push(".sdkman/candidates/java/");
                search_roots.push(sdkman);
            }

            for root in search_roots {
                if !root.exists() {
                    continue;
                }

                // If root itself is a JDK (e.g. Homebrew symlink)
                if self.collect_sdk_assets(&root).is_some() {
                    jdk_root = Some(root.clone());
                    break;
                }

                // If root is a parent directory containing multiple SDKs
                if let Ok(entries) = std::fs::read_dir(&root) {
                    for entry in entries.flatten() {
                        let mut sdk_path = entry.path();
                        if cfg!(target_os = "macos") && sdk_path.join("Contents/Home").exists() {
                            sdk_path = sdk_path.join("Contents/Home");
                        }
                        if self.collect_sdk_assets(&sdk_path).is_some() {
                            jdk_root = Some(sdk_path);
                            break;
                        }
                    }
                }

                if !self.cached_assets.is_empty() {
                    break;
                }
            }
        }

        // Update all entries with JDK source info
        if let Some(root) = jdk_root {
            let version = self.detect_jdk_version(&root);
            for entry in &mut self.cached_assets {
                entry.source = AssetSource::Jdk {
                    version: version.clone(),
                    path: root.clone(),
                };
            }
        }
    }

    fn detect_jdk_version(&self, jdk_root: &Path) -> Option<String> {
        // Try to read release file
        let release_file = jdk_root.join("release");
        if let Ok(content) = std::fs::read_to_string(&release_file) {
            for line in content.lines() {
                if line.starts_with("JAVA_VERSION=") {
                    let version = line
                        .trim_start_matches("JAVA_VERSION=")
                        .trim_matches('"')
                        .to_string();
                    return Some(version);
                }
            }
        }

        // Fallback: try to extract from path
        let path_str = jdk_root.to_string_lossy();
        if let Some(cap) = regex::Regex::new(r"jdk-?(\d+(?:\.\d+)*)")
            .ok()
            .and_then(|re| re.captures(&path_str))
        {
            return cap.get(1).map(|m| m.as_str().to_string());
        }

        None
    }

    fn collect_sdk_assets(&mut self, sdk_path: &Path) -> Option<()> {
        if !sdk_path.exists() {
            return None;
        }

        // Priority 1: Java 9+ Runtime Image (The most correct way for modern Java)
        let modules = sdk_path.join("lib/modules");
        if modules.exists() {
            self.cached_assets.push(AssetEntry::unknown(modules));
            return Some(());
        }

        // Priority 2: Java 8 Legacy Runtime
        let rt_jar = sdk_path.join("jre/lib/rt.jar");
        if rt_jar.exists() {
            self.cached_assets.push(AssetEntry::unknown(rt_jar));
            return Some(());
        }
        let lib_rt_jar = sdk_path.join("lib/rt.jar");
        if lib_rt_jar.exists() {
            self.cached_assets.push(AssetEntry::unknown(lib_rt_jar));
            return Some(());
        }

        // Priority 3: jmods (Fallback for some JDK builds without lib/modules)
        let jmods = sdk_path.join("jmods");
        if jmods.exists() {
            if let Ok(entries) = std::fs::read_dir(&jmods) {
                let mut found = false;
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                        self.cached_assets.push(AssetEntry::unknown(path));
                        found = true;
                    }
                }
                if found {
                    return Some(());
                }
            }
        }

        None
    }
}

impl Default for JdkDiscoverer {
    fn default() -> Self {
        Self::new()
    }
}

impl AssetDiscoverer for JdkDiscoverer {
    fn discover(&self) -> Box<dyn Iterator<Item = AssetEntry> + Send + '_> {
        Box::new(self.cached_assets.iter().cloned())
    }

    fn name(&self) -> &str {
        "JDK Discoverer"
    }

    fn source_type(&self) -> &str {
        "jdk"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jdk_discoverer_creation() {
        let discoverer = JdkDiscoverer::new();
        // Should not panic, may or may not find JDK depending on environment
        let assets: Vec<_> = discoverer.discover().collect();
        println!("Found {} JDK assets", assets.len());
        for asset in &assets {
            println!("  - {:?} from {:?}", asset.path, asset.source);
        }
    }

    #[test]
    fn test_collect_sdk_assets_java_11() {
        let temp = tempfile::tempdir().unwrap();
        let sdk_path = temp.path().to_path_buf();

        let modules_path = sdk_path.join("lib/modules");
        std::fs::create_dir_all(modules_path.parent().unwrap()).unwrap();
        std::fs::File::create(&modules_path).unwrap();

        let mut discoverer = JdkDiscoverer {
            cached_assets: Vec::new(),
        };
        let result = discoverer.collect_sdk_assets(&sdk_path);

        assert!(result.is_some());
        assert_eq!(discoverer.cached_assets.len(), 1);
        assert_eq!(discoverer.cached_assets[0].path, modules_path);
    }

    #[test]
    fn test_collect_sdk_assets_java_8() {
        let temp = tempfile::tempdir().unwrap();
        let sdk_path = temp.path().to_path_buf();

        let rt_jar_path = sdk_path.join("jre/lib/rt.jar");
        std::fs::create_dir_all(rt_jar_path.parent().unwrap()).unwrap();
        std::fs::File::create(&rt_jar_path).unwrap();

        let mut discoverer = JdkDiscoverer {
            cached_assets: Vec::new(),
        };
        let result = discoverer.collect_sdk_assets(&sdk_path);

        assert!(result.is_some());
        assert_eq!(discoverer.cached_assets.len(), 1);
        assert_eq!(discoverer.cached_assets[0].path, rt_jar_path);
    }
}
