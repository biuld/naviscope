use super::{LangResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphNode};
use crate::project::resolver::ResolvedUnit;
use crate::project::scanner::{ParsedContent, ParsedFile};

pub struct JavaResolver;

impl JavaResolver {
    pub fn new() -> Self {
        Self
    }

    /// Check if a node is top-level (class, interface, enum, annotation)
    fn is_top_level_node(&self, node: &GraphNode) -> bool {
        let kind = node.kind();
        kind == "class" || kind == "interface" || kind == "enum" || kind == "annotation"
    }
}

impl LangResolver for JavaResolver {
    fn resolve(&self, file: &ParsedFile, context: &ProjectContext) -> Result<ResolvedUnit> {
        let mut unit = ResolvedUnit::new();

        if let ParsedContent::Java(parse_result) = &file.content {
            // Step 1: Determine module
            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            // Step 2: Determine parent container (Package or Module)
            let container_id = if let Some(pkg_name) = &parse_result.package_name {
                let package_id = format!("{}::{}", module_id, pkg_name);

                // Add package node
                unit.add_node(
                    package_id.clone(),
                    GraphNode::gradle(
                        crate::model::lang::gradle::GradleElement::Package(
                            crate::model::lang::gradle::GradlePackage {
                                name: pkg_name.clone(),
                            },
                        ),
                        None,
                    ),
                );

                unit.add_edge(module_id, package_id.clone(), EdgeType::Contains);
                package_id
            } else {
                module_id
            };

            // Step 3: Add pre-built nodes and link to container
            for node in &parse_result.nodes {
                let fqn = node.fqn();
                unit.add_node(fqn.clone(), node.clone());

                if self.is_top_level_node(node) {
                    unit.add_edge(container_id.clone(), fqn, EdgeType::Contains);
                }
            }

            // Step 4: Add structural relations
            for (source_fqn, target_fqn, edge_type) in &parse_result.relations {
                unit.add_edge(source_fqn.clone(), target_fqn.clone(), edge_type.clone());
            }
        }

        Ok(unit)
    }
}
