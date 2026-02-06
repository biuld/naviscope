use naviscope_api::models::graph::{
    DisplaySymbolLocation, EdgeType, EmptyMetadata, GraphEdge, NodeKind, NodeSource,
};
use naviscope_api::models::symbol::{NodeId, Range};
use naviscope_plugin::{
    BuildResolver, IndexNode, ParsedContent, ParsedFile, ProjectContext, ResolvedUnit,
};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub struct GradleResolver;

impl GradleResolver {
    pub fn new() -> Self {
        Self
    }

    /// Standardizes a path to ensure consistency across different OS platforms and symlinks.
    fn normalize_path(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }

    /// Discovers built-in assets (SDK libraries like lib/modules, rt.jar or jmods).
    fn discover_builtin_assets(&self) -> Vec<PathBuf> {
        let mut assets = Vec::new();

        // 1. Check JAVA_HOME
        if let Ok(java_home) = std::env::var("JAVA_HOME") {
            self.collect_sdk_assets(Path::new(&java_home), &mut assets);
        }

        // 2. macOS specific: Use java_home tool
        #[cfg(target_os = "macos")]
        if assets.is_empty() {
            if let Ok(output) = std::process::Command::new("/usr/libexec/java_home").output() {
                if output.status.success() {
                    let path_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    self.collect_sdk_assets(Path::new(&path_str), &mut assets);
                }
            }
        }

        // 3. Search common installation paths
        if assets.is_empty() {
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
                self.collect_sdk_assets(&root, &mut assets);
                if !assets.is_empty() {
                    break;
                }

                // If root is a parent directory containing multiple SDKs
                if let Ok(entries) = std::fs::read_dir(root) {
                    for entry in entries.flatten() {
                        let mut sdk_path = entry.path();
                        if cfg!(target_os = "macos") && sdk_path.join("Contents/Home").exists() {
                            sdk_path.push("Contents/Home");
                        }
                        self.collect_sdk_assets(&sdk_path, &mut assets);
                        if !assets.is_empty() {
                            break;
                        }
                    }
                }

                if !assets.is_empty() {
                    break;
                }
            }
        }

        assets
    }

    fn collect_sdk_assets(&self, sdk_path: &Path, assets: &mut Vec<PathBuf>) {
        if !sdk_path.exists() {
            return;
        }

        // Priority 1: Java 9+ Runtime Image (The most correct way for modern Java)
        let modules = sdk_path.join("lib/modules");
        if modules.exists() {
            assets.push(modules);
            return;
        }

        // Priority 2: Java 8 Legacy Runtime
        let rt_jar = sdk_path.join("jre/lib/rt.jar");
        if rt_jar.exists() {
            assets.push(rt_jar);
            return;
        }
        let lib_rt_jar = sdk_path.join("lib/rt.jar");
        if lib_rt_jar.exists() {
            assets.push(lib_rt_jar);
            return;
        }

        // Priority 3: jmods (Fallback for some JDK builds without lib/modules)
        let jmods = sdk_path.join("jmods");
        if jmods.exists() {
            if let Ok(entries) = std::fs::read_dir(jmods) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("jmod") {
                        assets.push(path);
                    }
                }
            }
        }
    }

    /// Discovers external assets (third-party JARs).
    /// Discovers external assets (third-party JARs).
    fn discover_external_assets(&self, files: &[&ParsedFile]) -> Vec<PathBuf> {
        let mut assets = Vec::new();

        // 1. Fast Path: Scan .idea/libraries if available (IntelliJ projects)
        self.discover_from_idea(files, &mut assets);

        // 2. Accurate Path: Scan Gradle cache based on parsed dependencies
        self.discover_from_gradle_cache(files, &mut assets);

        assets.sort();
        assets.dedup();
        assets
    }

    fn discover_from_idea(&self, files: &[&ParsedFile], assets: &mut Vec<PathBuf>) {
        let Some(first_file) = files.first() else {
            return;
        };

        let mut current = first_file.file.path.clone();
        while let Some(parent) = current.parent() {
            let idea_libs = parent.join(".idea/libraries");
            if idea_libs.exists() {
                if let Ok(entries) = std::fs::read_dir(idea_libs) {
                    for entry in entries.flatten() {
                        self.parse_idea_lib_xml(&entry.path(), assets);
                    }
                }
                break;
            }
            current = parent.to_path_buf();
        }
    }

    fn parse_idea_lib_xml(&self, path: &Path, assets: &mut Vec<PathBuf>) {
        if path.extension().and_then(|e| e.to_str()) != Some("xml") {
            return;
        }

        let Ok(content) = std::fs::read_to_string(path) else {
            return;
        };

        for line in content.lines() {
            let Some(jar_path) = self.extract_jar_path_from_xml_line(line) else {
                continue;
            };

            if jar_path.exists() && jar_path.extension().and_then(|e| e.to_str()) == Some("jar") {
                assets.push(jar_path);
            }
        }
    }

    fn extract_jar_path_from_xml_line(&self, line: &str) -> Option<PathBuf> {
        if !line.contains("url=\"jar://") || !line.contains("!/\"") {
            return None;
        }

        let start = line.find("jar://")? + 6;
        let end = line.find("!/\"")?;
        let path_str = &line[start..end];

        if path_str.contains("$USER_HOME$") {
            dirs::home_dir().map(|home| {
                PathBuf::from(path_str.replace("$USER_HOME$", home.to_str().unwrap_or("")))
            })
        } else {
            Some(PathBuf::from(path_str))
        }
    }

    fn discover_from_gradle_cache(&self, files: &[&ParsedFile], assets: &mut Vec<PathBuf>) {
        let Some(mut cache_path) = dirs::home_dir() else {
            return;
        };
        cache_path.push(".gradle/caches/modules-2/files-2.1");

        if !cache_path.exists() {
            return;
        }

        for file in files {
            let ParsedContent::Metadata(value) = &file.content else {
                continue;
            };

            let Ok(res) = serde_json::from_value::<crate::model::GradleParseResult>(value.clone())
            else {
                continue;
            };

            for dep in res.dependencies {
                if dep.is_project {
                    continue;
                }
                if let (Some(group), Some(version)) = (dep.group, dep.version) {
                    let dep_dir = cache_path.join(&group).join(&dep.name).join(&version);
                    if dep_dir.exists() {
                        self.collect_jars_from_dir(&dep_dir, assets);
                    }
                }
            }
        }
    }

    fn collect_jars_from_dir(&self, dir: &Path, assets: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                self.collect_jars_from_dir(&path, assets);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jar") {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                if !name.ends_with("-sources.jar") && !name.ends_with("-javadoc.jar") {
                    assets.push(path);
                }
            }
        }
    }
}

impl BuildResolver for GradleResolver {
    fn resolve(
        &self,
        files: &[&ParsedFile],
    ) -> std::result::Result<(ResolvedUnit, ProjectContext), Box<dyn std::error::Error + Send + Sync>>
    {
        let mut unit = ResolvedUnit::new();
        let mut context = ProjectContext::new();

        // --- Step 0: Discover environment (Builtin & External) ---
        context.builtin_assets = self.discover_builtin_assets();
        context.external_assets = self.discover_external_assets(files);

        // --- Step 1: Discover all potential module paths ---
        let mut module_map: HashMap<PathBuf, ModuleData> = HashMap::new();

        for file in files {
            let dir_path = self.normalize_path(file.file.path.parent().unwrap());

            let data = module_map
                .entry(dir_path.clone())
                .or_insert_with(|| ModuleData {
                    build_file: None,
                    settings_file: None,
                });

            match &file.content {
                ParsedContent::Metadata(value) => {
                    // Try to deserialize as GradleParseResult first
                    if let Ok(gradle_result) =
                        serde_json::from_value::<crate::model::GradleParseResult>(value.clone())
                    {
                        data.build_file = Some((file, gradle_result));
                    } else if let Ok(settings) =
                        serde_json::from_value::<crate::model::GradleSettings>(value.clone())
                    {
                        data.settings_file = Some((file, settings));
                    }
                }
                ParsedContent::Unparsed(content_str) => {
                    let path = &file.file.path;
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name == "build.gradle" || name == "build.gradle.kts" {
                            if let Ok(deps) = crate::parser::parse_dependencies(content_str) {
                                let res = crate::model::GradleParseResult { dependencies: deps };
                                data.build_file = Some((file, res));
                            }
                        } else if name == "settings.gradle" || name == "settings.gradle.kts" {
                            if let Ok(settings) = crate::parser::parse_settings(content_str) {
                                data.settings_file = Some((file, settings));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        if module_map.is_empty() {
            return Ok((unit, context));
        }

        // --- Step 2: Identify the Global Root ---
        let mut sorted_paths: Vec<PathBuf> = module_map.keys().cloned().collect();
        sorted_paths.sort_by_key(|p| p.components().count());

        let root_path = sorted_paths
            .iter()
            .find(|p| {
                module_map
                    .get(*p)
                    .and_then(|m| m.settings_file.as_ref())
                    .is_some()
            })
            .cloned()
            .unwrap_or_else(|| sorted_paths[0].clone());

        // --- Step 3: Create Project Node ---
        let root_info = module_map.get(&root_path).unwrap();

        let project_name = if let Some((_, settings)) = &root_info.settings_file {
            settings
                .root_project_name
                .as_ref()
                .map(|n| n.trim_matches(|c| c == '\"' || c == '\'').to_string())
                .unwrap_or_else(|| {
                    root_path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string()
                })
        } else {
            root_path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string()
        };

        let project_id_str = format!("project:{}", project_name);
        let project_id = NodeId::Flat(project_id_str.clone());

        // Add Project node
        unit.add_node(IndexNode {
            id: project_id.clone(),
            name: project_name.clone(),
            kind: NodeKind::Project,
            lang: "gradle".to_string(),
            source: NodeSource::Project,
            status: naviscope_api::models::graph::ResolutionStatus::Resolved,
            location: Some(DisplaySymbolLocation {
                path: root_path.to_string_lossy().to_string(),
                range: Range {
                    start_line: 0,
                    start_col: 0,
                    end_line: 0,
                    end_col: 0,
                },
                selection_range: None,
            }),
            metadata: Arc::new(EmptyMetadata),
        });

        // --- Step 4: Assign Module IDs ---
        let mut path_to_id: HashMap<PathBuf, NodeId> = HashMap::new();

        for path in &sorted_paths {
            let id_str = if path == &root_path {
                // Root module is now a child of project
                format!("{}::module:{}", project_id_str, project_name)
            } else if path.starts_with(&root_path) {
                let rel = path.strip_prefix(&root_path).unwrap();
                let logical = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                format!("{}::module:{}", project_id_str, logical)
            } else {
                // External modules (e.g., buildSrc)
                format!(
                    "{}::module:{}",
                    project_id_str,
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            };
            path_to_id.insert(path.clone(), NodeId::Flat(id_str));
        }

        // --- Step 5: Construct Module Nodes and Hierarchy ---
        let root_module_id = path_to_id.get(&root_path).unwrap();

        // Add root module and link to project
        {
            let data = module_map.get(&root_path).unwrap();
            let root_module_id_str = root_module_id.to_string();
            let display_name = root_module_id_str
                .split("::module:")
                .nth(1)
                .unwrap_or(&project_name);

            unit.add_node(IndexNode {
                id: root_module_id.clone(),
                name: display_name.to_string(),
                kind: NodeKind::Module,
                lang: "gradle".to_string(),
                source: NodeSource::Project,
                status: naviscope_api::models::graph::ResolutionStatus::Resolved,
                location: data
                    .build_file
                    .as_ref()
                    .map(|(f, _)| f.file.path.clone())
                    .or_else(|| {
                        data.settings_file
                            .as_ref()
                            .map(|(f, _)| f.file.path.clone())
                    })
                    .map(|path| DisplaySymbolLocation {
                        path: path.to_string_lossy().to_string(),
                        range: Range {
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        },
                        selection_range: None,
                    }),
                metadata: Arc::new(EmptyMetadata),
            });

            unit.add_edge(
                project_id.clone(),
                root_module_id.clone(),
                GraphEdge::new(EdgeType::Contains),
            );

            context
                .path_to_module
                .insert(root_path.clone(), root_module_id.to_string());
        }

        // Add other modules
        for path in &sorted_paths {
            if path == &root_path {
                continue;
            }

            let data = module_map.get(path).unwrap();
            let id = path_to_id.get(path).unwrap();
            let id_str = id.to_string();
            let display_name = id_str.split("::module:").nth(1).unwrap_or(&id_str);

            unit.add_node(IndexNode {
                id: id.clone(),
                name: display_name.to_string(),
                kind: NodeKind::Module,
                lang: "gradle".to_string(),
                source: NodeSource::Project,
                status: naviscope_api::models::graph::ResolutionStatus::Resolved,
                location: data
                    .build_file
                    .as_ref()
                    .map(|(f, _)| f.file.path.clone())
                    .or_else(|| {
                        data.settings_file
                            .as_ref()
                            .map(|(f, _)| f.file.path.clone())
                    })
                    .map(|path| DisplaySymbolLocation {
                        path: path.to_string_lossy().to_string(),
                        range: Range {
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        },
                        selection_range: None,
                    }),
                metadata: Arc::new(EmptyMetadata),
            });

            context.path_to_module.insert(path.clone(), id.to_string());

            // Establish hierarchy
            let mut found_parent = false;
            let mut current = path.parent();

            while let Some(p) = current {
                let normalized_p = self.normalize_path(p);
                if let Some(parent_id) = path_to_id.get(&normalized_p) {
                    unit.add_edge(
                        parent_id.clone(),
                        id.clone(),
                        GraphEdge::new(EdgeType::Contains),
                    );
                    found_parent = true;
                    break;
                }
                if normalized_p == root_path {
                    break;
                }
                current = p.parent();
            }

            // Fallback: link to root module if no parent found
            if !found_parent && path.starts_with(&root_path) {
                unit.add_edge(
                    root_module_id.clone(),
                    id.clone(),
                    GraphEdge::new(EdgeType::Contains),
                );
            }
        }

        // --- Step 6: Build Dependencies ---
        for path in &sorted_paths {
            let data = module_map.get(path).unwrap();
            let id = path_to_id.get(path).unwrap();

            if let Some((_, content)) = &data.build_file {
                for dep in &content.dependencies {
                    let target_id_str = if dep.is_project {
                        let clean_name = dep
                            .name
                            .trim_matches(|c| c == ':' || c == '\"' || c == '\'')
                            .replace(':', "/");
                        format!("{}::module:{}", project_id_str, clean_name)
                    } else {
                        let group = dep.group.as_deref().unwrap_or("");
                        let version = dep.version.as_deref().unwrap_or("");
                        format!("dep:{}:{}:{}", group, dep.name, version)
                    };
                    let target_id = NodeId::Flat(target_id_str);

                    if !dep.is_project {
                        unit.add_node(IndexNode {
                            id: target_id.clone(),
                            name: dep.name.clone(),
                            kind: NodeKind::Dependency,
                            lang: "gradle".to_string(),
                            source: NodeSource::External,
                            status: naviscope_api::models::graph::ResolutionStatus::Resolved,
                            location: Some(DisplaySymbolLocation {
                                path: data
                                    .build_file
                                    .as_ref()
                                    .unwrap()
                                    .0
                                    .file
                                    .path
                                    .to_string_lossy()
                                    .to_string(),
                                range: Range {
                                    start_line: 0,
                                    start_col: 0,
                                    end_line: 0,
                                    end_col: 0,
                                },
                                selection_range: None,
                            }),
                            metadata: Arc::new(EmptyMetadata),
                        });
                    }

                    unit.add_edge(
                        id.clone(),
                        target_id,
                        GraphEdge::new(EdgeType::UsesDependency),
                    );
                }
            }
        }

        Ok((unit, context))
    }
}

struct ModuleData<'a> {
    build_file: Option<(&'a ParsedFile, crate::model::GradleParseResult)>,
    settings_file: Option<(&'a ParsedFile, crate::model::GradleSettings)>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_plugin::{GraphOp, SourceFile};

    fn create_mock_file(path: &str, content: ParsedContent) -> ParsedFile {
        ParsedFile {
            file: SourceFile {
                path: PathBuf::from(path),
                content_hash: 0,
                last_modified: 0,
            },
            content,
        }
    }

    #[test]
    fn test_resolve_robust_hierarchy() {
        let resolver = GradleResolver::new();

        let root_settings = create_mock_file(
            "/repo/settings.gradle",
            ParsedContent::Metadata(
                serde_json::to_value(crate::model::GradleSettings {
                    root_project_name: Some("spring-boot-build".to_string()),
                    included_projects: vec![],
                })
                .unwrap(),
            ),
        );
        let sub_project_build = create_mock_file(
            "/repo/spring-boot-project/build.gradle",
            ParsedContent::Metadata(
                serde_json::to_value(crate::model::GradleParseResult {
                    dependencies: vec![],
                })
                .unwrap(),
            ),
        );
        let core_build = create_mock_file(
            "/repo/spring-boot-project/spring-boot/build.gradle",
            ParsedContent::Metadata(
                serde_json::to_value(crate::model::GradleParseResult {
                    dependencies: vec![],
                })
                .unwrap(),
            ),
        );

        let files = vec![&root_settings, &sub_project_build, &core_build];
        let (unit, _) = resolver.resolve(&files).unwrap();

        let edges: Vec<_> = unit
            .ops
            .iter()
            .filter_map(|op| {
                if let GraphOp::AddEdge {
                    from_id,
                    to_id,
                    edge,
                } = op
                {
                    if edge.edge_type == EdgeType::Contains {
                        let from = from_id.to_string();
                        let to = to_id.to_string();
                        // Strip quotes if they exist (NodeId Display adds them for Flat)
                        let clean_from = from.trim_matches('\"');
                        let clean_to = to.trim_matches('\"');
                        Some((clean_from.to_string(), clean_to.to_string()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Should have: project -> root_module -> sub_modules
        assert!(edges.iter().any(|(f, t)| f == "project:spring-boot-build"
            && t == "project:spring-boot-build::module:spring-boot-build"));
        assert!(edges.iter().any(|(f, t)| f
            == "project:spring-boot-build::module:spring-boot-build"
            && t == "project:spring-boot-build::module:spring-boot-project"));
        assert!(edges.iter().any(|(f, t)| f
            == "project:spring-boot-build::module:spring-boot-project"
            && t == "project:spring-boot-build::module:spring-boot-project/spring-boot"));
    }

    #[test]
    fn test_discover_external_assets_from_idea() {
        let resolver = GradleResolver::new();
        let temp = tempfile::tempdir().unwrap();
        let idea_libs = temp.path().join(".idea/libraries");
        std::fs::create_dir_all(&idea_libs).unwrap();

        let jar_path = temp.path().join("mock.jar");
        std::fs::File::create(&jar_path).unwrap();

        let lib_xml = idea_libs.join("mock_lib.xml");
        let xml_content = format!(
            r#"
<component name="libraryTable">
  <library name="Gradle: mock-lib">
    <CLASSES>
      <root url="jar://{}!/" />
    </CLASSES>
  </library>
</component>
"#,
            jar_path.display()
        );
        std::fs::write(&lib_xml, xml_content).unwrap();

        let mock_file = create_mock_file(
            temp.path().join("build.gradle").to_str().unwrap(),
            ParsedContent::Unparsed("".to_string()),
        );
        let assets: Vec<_> = resolver
            .discover_external_assets(&[&mock_file])
            .into_iter()
            .map(|p| p.canonicalize().unwrap())
            .collect();

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0], jar_path.canonicalize().unwrap());
    }

    #[test]
    fn test_discover_builtin_assets_java_11() {
        let resolver = GradleResolver::new();
        let temp = tempfile::tempdir().unwrap();
        let sdk_path = temp.path().to_path_buf();

        let modules_path = sdk_path.join("lib/modules");
        std::fs::create_dir_all(modules_path.parent().unwrap()).unwrap();
        std::fs::File::create(&modules_path).unwrap();

        let mut assets = Vec::new();
        resolver.collect_sdk_assets(&sdk_path, &mut assets);

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0], modules_path);
    }

    #[test]
    fn test_discover_builtin_assets_java_8() {
        let resolver = GradleResolver::new();
        let temp = tempfile::tempdir().unwrap();
        let sdk_path = temp.path().to_path_buf();

        let rt_jar_path = sdk_path.join("jre/lib/rt.jar");
        std::fs::create_dir_all(rt_jar_path.parent().unwrap()).unwrap();
        std::fs::File::create(&rt_jar_path).unwrap();

        let mut assets = Vec::new();
        resolver.collect_sdk_assets(&sdk_path, &mut assets);

        assert_eq!(assets.len(), 1);
        assert_eq!(assets[0], rt_jar_path);
    }
}
