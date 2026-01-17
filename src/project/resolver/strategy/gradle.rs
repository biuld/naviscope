use super::{BuildResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphNode};
use crate::model::lang::gradle::{GradleElement, GradlePackage};
use crate::project::resolver::ResolvedUnit;
use crate::project::scanner::{ParsedContent, ParsedFile};

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

        for file in files {
            if let ParsedContent::Gradle(parse_result) = &file.content {
                // Get the directory containing the build.gradle file
                let parent_path = file.file.path.parent().unwrap().to_path_buf();
                
                // Infer module name from the directory name
                let module_name = parent_path
                    .file_name()
                    .and_then(|s| s.to_str())
                    .map(|s| format!(":{}", s))
                    .unwrap_or_else(|| ":root".to_string());

                let module_id = format!("module:{}", module_name);
                
                // Add to context for phase 2
                context.path_to_module.insert(parent_path, module_id.clone());

                // Add module node
                unit.add_node(
                    module_id.clone(),
                    GraphNode::gradle(
                        GradleElement::Package(GradlePackage {
                            name: module_name.clone(),
                        }),
                        Some(file.file.path.clone()),
                    ),
                );

                // Add dependencies
                for dep in &parse_result.dependencies {
                    let dep_id = format!("dep:{}:{}:{}", dep.group, dep.name, dep.version);
                    unit.add_node(
                        dep_id.clone(),
                        GraphNode::gradle(
                            GradleElement::Dependency(dep.clone()),
                            Some(file.file.path.clone()),
                        ),
                    );

                    // Link: Module uses this dependency
                    unit.add_edge(module_id.clone(), dep_id, EdgeType::UsesDependency);
                }
            }
        }

        Ok((unit, context))
    }
}
