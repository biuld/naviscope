use crate::resolver::{BuildResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode, ResolvedUnit};
use crate::model::lang::gradle::{GradleElement, GradleModule};
use crate::project::scanner::{ParsedContent, ParsedFile};
use std::collections::HashMap;

pub struct GradleResolver;

impl GradleResolver {
    pub fn new() -> Self {
        Self
    }
}

impl BuildResolver for GradleResolver {
    fn resolve(&self, files: &[&ParsedFile]) -> Result<(ResolvedUnit, ProjectContext)> {
        let mut unit = ResolvedUnit::new();
        let mut context = ProjectContext::new();

        // 1. First pass: identify root and all modules
        let settings_file = files
            .iter()
            .find(|f| matches!(f.content, ParsedContent::GradleSettings(_)));

        let mut root_name = "root".to_string();
        let mut root_path = std::path::PathBuf::new();
        let mut included_projects = Vec::new();

        if let Some(f) = settings_file {
            if let ParsedContent::GradleSettings(s) = &f.content {
                if let Some(name) = &s.root_project_name {
                    root_name = name.clone();
                }
                root_path = f.file.path.parent().unwrap().to_path_buf();
                included_projects = s.included_projects.clone();
            }
        } else if let Some(first) = files.first() {
            root_path = first.file.path.parent().unwrap().to_path_buf();
            root_name = root_path.file_name().and_then(|s| s.to_str()).unwrap_or("root").to_string();
        }

        let root_module_id = "module::root".to_string();
        context.path_to_module.insert(root_path.clone(), root_module_id.clone());

        // Create root node
        unit.add_node(
            root_module_id.clone(),
            GraphNode::gradle(
                GradleElement::Module(GradleModule {
                    name: root_name.clone(),
                    id: root_module_id.clone(),
                }),
                None,
            ),
        );

        // Pre-create all included modules to ensure nodes exist before edges
        let mut module_to_path = HashMap::new();
        module_to_path.insert(":".to_string(), root_path.clone());

        for project_path in &included_projects {
            let mut current_name = String::new();
            let mut current_fs_path = root_path.clone();
            
            for part in project_path.split(':') {
                if part.is_empty() { continue; }
                
                let parent_name = if current_name.is_empty() { ":".to_string() } else { current_name.clone() };
                current_name = format!("{}:{}", current_name, part);
                current_fs_path.push(part);
                
                let current_id = format!("module:{}", current_name);
                let parent_id = if parent_name == ":" { "module::root".to_string() } else { format!("module:{}", parent_name) };
                
                // Pre-create node
                unit.add_node(
                    current_id.clone(),
                    GraphNode::gradle(
                        GradleElement::Module(GradleModule {
                            name: current_name.clone(),
                            id: current_id.clone(),
                        }),
                        None, // Will be updated if build.gradle is found
                    ),
                );
                
                unit.add_edge(parent_id, current_id.clone(), GraphEdge::new(EdgeType::Contains));
                
                if current_name == format!(":{}", project_path.trim_start_matches(':')) {
                    module_to_path.insert(current_name.clone(), current_fs_path.clone());
                }
            }
        }

        // 2. Second pass: process build.gradle files to add detailed info and dependencies
        for file in files {
            if let ParsedContent::Gradle(parse_result) = &file.content {
                let current_fs_path = file.file.path.parent().unwrap();
                
                let module_name = module_to_path
                    .iter()
                    .find(|(_, path)| *path == current_fs_path)
                    .map(|(name, _)| name.clone())
                    .unwrap_or_else(|| {
                        format!(":{}", current_fs_path.file_name().unwrap().to_str().unwrap())
                    });

                let module_id = if module_name == ":" {
                    "module::root".to_string()
                } else {
                    format!("module:{}", module_name)
                };
                
                context.path_to_module.insert(current_fs_path.to_path_buf(), module_id.clone());

                // Update node with file path (AddNode with same ID updates it)
                unit.add_node(
                    module_id.clone(),
                    GraphNode::gradle(
                        GradleElement::Module(GradleModule {
                            name: if module_name == ":" { root_name.clone() } else { module_name.clone() },
                            id: module_id.clone(),
                        }),
                        Some(file.file.path.clone()),
                    ),
                );

                for dep in &parse_result.dependencies {
                    if dep.is_project {
                        let target_module_name = if dep.name.starts_with(':') {
                            dep.name.clone()
                        } else {
                            format!("{}:{}", module_name, dep.name)
                        };
                        let target_id = if target_module_name == ":" { "module::root".to_string() } else { format!("module:{}", target_module_name) };
                        unit.add_edge(module_id.clone(), target_id, GraphEdge::new(EdgeType::UsesDependency));
                    } else {
                        let group = dep.group.as_deref().unwrap_or("");
                        let version = dep.version.as_deref().unwrap_or("");
                        let dep_id = format!("dep:{}:{}:{}", group, dep.name, version);
                        
                        let mut dep_node = dep.clone();
                        dep_node.id = dep_id.clone();
                        
                        unit.add_node(
                            dep_id.clone(),
                            GraphNode::gradle(
                                GradleElement::Dependency(dep_node),
                                Some(file.file.path.clone()),
                            ),
                        );

                        unit.add_edge(module_id.clone(), dep_id, GraphEdge::new(EdgeType::UsesDependency));
                    }
                }
            }
        }

        Ok((unit, context))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::lang::gradle::{GradleDependency, GradleSettings, GradleParseResult};
    use crate::project::source::SourceFile;
    use crate::model::graph::GraphOp;
    use std::path::PathBuf;

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
    fn test_resolve_multi_module_hierarchy() {
        let resolver = GradleResolver::new();
        
        let settings = GradleSettings {
            root_project_name: Some("my-project".to_string()),
            included_projects: vec!["core".to_string(), "core:api".to_string()],
        };
        let settings_file = create_mock_file("/repo/settings.gradle", ParsedContent::GradleSettings(settings));

        let root_build = create_mock_file("/repo/build.gradle", ParsedContent::Gradle(GradleParseResult { dependencies: vec![] }));
        let core_build = create_mock_file("/repo/core/build.gradle", ParsedContent::Gradle(GradleParseResult { dependencies: vec![] }));
        let api_build = create_mock_file("/repo/core/api/build.gradle", ParsedContent::Gradle(GradleParseResult {
            dependencies: vec![
                GradleDependency {
                    group: None,
                    name: ":core".to_string(),
                    version: None,
                    is_project: true,
                    id: String::new(),
                }
            ]
        }));

        let files = vec![&settings_file, &root_build, &core_build, &api_build];
        let (unit, _) = resolver.resolve(&files).unwrap();

        let node_ids: Vec<_> = unit.ops.iter().filter_map(|op| {
            if let GraphOp::AddNode { id, .. } = op { Some(id.clone()) } else { None }
        }).collect();

        assert!(node_ids.contains(&"module::root".to_string()));
        assert!(node_ids.contains(&"module::core".to_string()));
        assert!(node_ids.contains(&"module::core:api".to_string()));

        let contains_edges: Vec<_> = unit.ops.iter().filter_map(|op| {
            if let GraphOp::AddEdge { from_id, to_id, edge } = op {
                if edge.edge_type == EdgeType::Contains { Some((from_id.clone(), to_id.clone())) } else { None }
            } else { None }
        }).collect();

        assert!(contains_edges.contains(&("module::root".to_string(), "module::core".to_string())));
        assert!(contains_edges.contains(&("module::core".to_string(), "module::core:api".to_string())));

        let dep_edges: Vec<_> = unit.ops.iter().filter_map(|op| {
            if let GraphOp::AddEdge { from_id, to_id, edge } = op {
                if edge.edge_type == EdgeType::UsesDependency { Some((from_id.clone(), to_id.clone())) } else { None }
            } else { None }
        }).collect();

        assert!(dep_edges.contains(&("module::core:api".to_string(), "module::core".to_string())));
    }

    #[test]
    fn test_resolve_external_dependencies() {
        let resolver = GradleResolver::new();
        
        let build_file = create_mock_file("/repo/build.gradle", ParsedContent::Gradle(GradleParseResult {
            dependencies: vec![
                GradleDependency {
                    group: Some("com.google.guava".to_string()),
                    name: "guava".to_string(),
                    version: Some("31.1-jre".to_string()),
                    is_project: false,
                    id: String::new(),
                }
            ]
        }));

        let files = vec![&build_file];
        let (unit, _) = resolver.resolve(&files).unwrap();

        let dep_id = "dep:com.google.guava:guava:31.1-jre".to_string();
        
        let has_dep_node = unit.ops.iter().any(|op| {
            if let GraphOp::AddNode { id, .. } = op { id == &dep_id } else { false }
        });
        assert!(has_dep_node);

        let has_edge = unit.ops.iter().any(|op| {
            if let GraphOp::AddEdge { from_id, to_id, edge } = op {
                from_id == "module::root" && to_id == &dep_id && edge.edge_type == EdgeType::UsesDependency
            } else { false }
        });
        assert!(has_edge);
    }
}
