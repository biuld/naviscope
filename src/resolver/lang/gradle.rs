use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode, ResolvedUnit};
use crate::model::lang::gradle::{GradleElement, GradleModule};
use crate::project::scanner::{ParsedContent, ParsedFile};
use crate::resolver::{BuildResolver, ProjectContext};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub struct GradleResolver;

impl GradleResolver {
    pub fn new() -> Self {
        Self
    }

    /// Standardizes a path to ensure consistency across different OS platforms and symlinks.
    fn normalize_path(&self, path: &Path) -> PathBuf {
        path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
    }
}

impl BuildResolver for GradleResolver {
    fn resolve(&self, files: &[&ParsedFile]) -> Result<(ResolvedUnit, ProjectContext)> {
        let mut unit = ResolvedUnit::new();
        let mut context = ProjectContext::new();

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
                ParsedContent::Gradle(content) => {
                    data.build_file = Some((file, content));
                }
                ParsedContent::GradleSettings(content) => {
                    data.settings_file = Some((file, content));
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
            .find(|p| module_map.get(*p).and_then(|m| m.settings_file).is_some())
            .cloned()
            .unwrap_or_else(|| sorted_paths[0].clone());

        // --- Step 3: Create Project Node ---
        let root_info = module_map.get(&root_path).unwrap();

        let project_name = if let Some((_, settings)) = root_info.settings_file {
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

        let project_id = format!("project:{}", project_name);

        // Add Project node
        unit.add_node(
            project_id.clone(),
            GraphNode::project(
                project_id.clone(),
                root_path.clone(),
                crate::model::graph::BuildSystem::Gradle,
            ),
        );

        // --- Step 4: Assign Module IDs ---
        let mut path_to_id: HashMap<PathBuf, String> = HashMap::new();

        for path in &sorted_paths {
            let id = if path == &root_path {
                // Root module is now a child of project
                format!("{}::module:{}", project_id, project_name)
            } else if path.starts_with(&root_path) {
                let rel = path.strip_prefix(&root_path).unwrap();
                let logical = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(":");
                format!("{}::module:{}", project_id, logical)
            } else {
                // External modules (e.g., buildSrc)
                format!(
                    "{}::module:{}",
                    project_id,
                    path.file_name().unwrap_or_default().to_string_lossy()
                )
            };
            path_to_id.insert(path.clone(), id);
        }

        // --- Step 5: Construct Module Nodes and Hierarchy ---
        let root_module_id = path_to_id.get(&root_path).unwrap();

        // Add root module and link to project
        {
            let data = module_map.get(&root_path).unwrap();
            let display_name = root_module_id
                .split("::module:")
                .nth(1)
                .unwrap_or(&project_name);

            unit.add_node(
                root_module_id.clone(),
                GraphNode::gradle(
                    GradleElement::Module(GradleModule {
                        name: display_name.to_string(),
                        id: root_module_id.clone(),
                    }),
                    data.build_file
                        .as_ref()
                        .map(|(f, _)| f.file.path.clone())
                        .or_else(|| {
                            data.settings_file
                                .as_ref()
                                .map(|(f, _)| f.file.path.clone())
                        }),
                ),
            );

            unit.add_edge(
                project_id.clone(),
                root_module_id.clone(),
                GraphEdge::new(EdgeType::Contains),
            );

            context
                .path_to_module
                .insert(root_path.clone(), root_module_id.clone());
        }

        // Add other modules
        for path in &sorted_paths {
            if path == &root_path {
                continue;
            }

            let data = module_map.get(path).unwrap();
            let id = path_to_id.get(path).unwrap();
            let display_name = id.split("::module:").nth(1).unwrap_or(id);

            unit.add_node(
                id.clone(),
                GraphNode::gradle(
                    GradleElement::Module(GradleModule {
                        name: display_name.to_string(),
                        id: id.clone(),
                    }),
                    data.build_file
                        .as_ref()
                        .map(|(f, _)| f.file.path.clone())
                        .or_else(|| {
                            data.settings_file
                                .as_ref()
                                .map(|(f, _)| f.file.path.clone())
                        }),
                ),
            );

            context.path_to_module.insert(path.clone(), id.clone());

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

            if let Some((_, content)) = data.build_file {
                for dep in &content.dependencies {
                    let target_id = if dep.is_project {
                        let clean_name = dep
                            .name
                            .trim_matches(|c| c == ':' || c == '\"' || c == '\'');
                        format!("{}::module:{}", project_id, clean_name)
                    } else {
                        let group = dep.group.as_deref().unwrap_or("");
                        let version = dep.version.as_deref().unwrap_or("");
                        format!("dep:{}:{}:{}", group, dep.name, version)
                    };

                    if !dep.is_project {
                        let mut dep_node = dep.clone();
                        dep_node.id = target_id.clone();
                        unit.add_node(
                            target_id.clone(),
                            GraphNode::gradle(
                                GradleElement::Dependency(dep_node),
                                Some(data.build_file.unwrap().0.file.path.clone()),
                            ),
                        );
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
    build_file: Option<(
        &'a ParsedFile,
        &'a crate::model::lang::gradle::GradleParseResult,
    )>,
    settings_file: Option<(
        &'a ParsedFile,
        &'a crate::model::lang::gradle::GradleSettings,
    )>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::graph::GraphOp;
    use crate::model::lang::gradle::{GradleParseResult, GradleSettings};
    use crate::project::source::SourceFile;

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
            ParsedContent::GradleSettings(GradleSettings {
                root_project_name: Some("spring-boot-build".to_string()),
                included_projects: vec![],
            }),
        );
        let sub_project_build = create_mock_file(
            "/repo/spring-boot-project/build.gradle",
            ParsedContent::Gradle(GradleParseResult {
                dependencies: vec![],
            }),
        );
        let core_build = create_mock_file(
            "/repo/spring-boot-project/spring-boot/build.gradle",
            ParsedContent::Gradle(GradleParseResult {
                dependencies: vec![],
            }),
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
                        Some((from_id.as_str(), to_id.as_str()))
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        // Should have: project -> root_module -> sub_modules
        assert!(edges.contains(&(
            "project:spring-boot-build",
            "project:spring-boot-build::module:spring-boot-build"
        )));
        assert!(edges.contains(&(
            "project:spring-boot-build::module:spring-boot-build",
            "project:spring-boot-build::module:spring-boot-project"
        )));
        assert!(edges.contains(&(
            "project:spring-boot-build::module:spring-boot-project",
            "project:spring-boot-build::module:spring-boot-project:spring-boot"
        )));
    }
}
