use super::{LangResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode};
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
            // Build import map for better FQN resolution
            let mut import_map = std::collections::HashMap::new();
            for imp in &parse_result.imports {
                if let Some(last_dot) = imp.rfind('.') {
                    let short_name = &imp[last_dot + 1..];
                    import_map.insert(short_name, imp);
                }
            }

            // Step 1: Determine module
            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            // Step 2: Determine parent container (Package or Module)
            let (container_id, current_pkg_prefix) = if let Some(pkg_name) = &parse_result.package_name {
                let package_id = format!("{}::{}", module_id, pkg_name);
                let pkg_prefix = format!("{}.", pkg_name);

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

                unit.add_edge(module_id, package_id.clone(), GraphEdge::new(EdgeType::Contains));
                (package_id, Some(pkg_prefix))
            } else {
                (module_id, None)
            };

            // Step 3: Add pre-built nodes and link to container
            for node in &parse_result.nodes {
                let fqn = node.fqn();
                unit.add_node(fqn.clone(), node.clone());

                if self.is_top_level_node(node) {
                    unit.add_edge(container_id.clone(), fqn, GraphEdge::new(EdgeType::Contains));
                }
            }

            // Step 4: Add relations with FQN resolution
            for (source_fqn, target_fqn, edge_type, range) in &parse_result.relations {
                let mut resolved_target = target_fqn.clone();

                // Simple heuristic for FQN resolution
                if !target_fqn.contains('.') {
                    if let Some(full_fqn) = import_map.get(target_fqn.as_str()) {
                        resolved_target = full_fqn.to_string();
                    } else if let Some(prefix) = &current_pkg_prefix {
                        // Assume it might be in the same package (best effort)
                        resolved_target = format!("{}{}", prefix, target_fqn);
                    }
                }

                let mut edge = GraphEdge::new(edge_type.clone());
                edge.range = *range;
                unit.add_edge(source_fqn.clone(), resolved_target, edge);
            }
        }

        Ok(unit)
    }
}
