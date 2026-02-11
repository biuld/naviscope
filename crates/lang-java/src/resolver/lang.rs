use crate::inference::adapters::HeuristicAdapter;
use crate::inference::{TypeProvider, TypeResolutionContext};
use crate::model::JavaIndexMetadata;
use crate::resolver::JavaResolver;
use crate::resolver::context::ResolutionContext;
use naviscope_api::models::graph::{EdgeType, GraphEdge, NodeKind};
use naviscope_api::models::symbol::{NodeId, SymbolResolution, TypeRef};
use naviscope_plugin::{
    GraphOp, IndexNode, ParsedContent, ParsedFile, ProjectContext, ResolvedUnit, SourceIndexCap,
};
use std::sync::Arc;

impl SourceIndexCap for JavaResolver {
    fn compile_source(
        &self,
        file: &ParsedFile,
        context: &ProjectContext,
    ) -> std::result::Result<ResolvedUnit, Box<dyn std::error::Error + Send + Sync>> {
        let mut unit = ResolvedUnit::new();
        let dummy_index = naviscope_plugin::EmptyCodeGraph;
        let type_provider = HeuristicAdapter;

        let parse_result_owned;
        let parse_result = match &file.content {
            ParsedContent::Language(res) => res,
            ParsedContent::Unparsed(src) => {
                if file.path().extension().map_or(false, |e| e == "java") {
                    parse_result_owned = self.parser.parse_file(src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Ok(unit);
                }
            }
            ParsedContent::Lazy => {
                if file.path().extension().map_or(false, |e| e == "java") {
                    let src = std::fs::read_to_string(file.path()).map_err(|e| {
                        format!("Failed to read file {}: {}", file.path().display(), e)
                    })?;
                    parse_result_owned = self.parser.parse_file(&src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Ok(unit);
                }
            }
            _ => return Ok(unit),
        };

        {
            unit.identifiers = parse_result.output.identifiers.clone();
            unit.ops.push(GraphOp::UpdateIdentifiers {
                path: Arc::from(file.file.path.as_path()),
                identifiers: unit.identifiers.clone(),
            });

            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            let container_id = if let Some(pkg_name) = &parse_result.package_name {
                let package_parts: Vec<_> = pkg_name
                    .split('.')
                    .map(|s| {
                        (
                            naviscope_api::models::graph::NodeKind::Package,
                            s.to_string(),
                        )
                    })
                    .collect();
                let package_id = NodeId::Structured(package_parts);

                let package_node = IndexNode {
                    id: package_id.clone(),
                    name: pkg_name.to_string(),
                    kind: NodeKind::Package,
                    lang: "java".to_string(),
                    source: naviscope_api::models::graph::NodeSource::Project,
                    status: naviscope_api::models::graph::ResolutionStatus::Resolved,
                    location: None,
                    metadata: Arc::new(JavaIndexMetadata::Package),
                };

                unit.add_node(package_node);

                unit.add_edge(
                    module_id.clone().into(),
                    package_id.clone(),
                    GraphEdge::new(EdgeType::Contains),
                );

                package_id
            } else {
                module_id.into()
            };

            let mut known_types = std::collections::HashSet::<String>::new();
            let mut local_type_map = std::collections::HashMap::<String, String>::new();

            for node in &parse_result.output.nodes {
                if matches!(
                    node.kind,
                    NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation
                ) {
                    known_types.insert(node.id.to_string());
                }
            }

            let res_ctx = TypeResolutionContext {
                package: parse_result.package_name.clone(),
                imports: parse_result.imports.clone(),
                type_parameters: Vec::new(),
                known_fqns: known_types.iter().cloned().collect(),
            };

            for node in &parse_result.output.nodes {
                let mut node = node.clone();

                if let Some(java_idx_meta) =
                    node.metadata.as_any().downcast_ref::<JavaIndexMetadata>()
                {
                    let mut element = java_idx_meta.clone();

                    match &mut element {
                        JavaIndexMetadata::Method {
                            return_type,
                            parameters,
                            ..
                        } => {
                            *return_type =
                                self.resolve_type_ref(return_type, &type_provider, &res_ctx);
                            for param in parameters {
                                param.type_ref = self.resolve_type_ref(
                                    &param.type_ref,
                                    &type_provider,
                                    &res_ctx,
                                );
                                if let TypeRef::Id(type_fqn) = &param.type_ref {
                                    local_type_map.insert(node.name.clone(), type_fqn.clone());
                                }
                            }
                        }
                        JavaIndexMetadata::Field { type_ref, .. } => {
                            *type_ref = self.resolve_type_ref(type_ref, &type_provider, &res_ctx);
                            if let TypeRef::Id(type_fqn) = &type_ref {
                                local_type_map.insert(node.name.clone(), type_fqn.clone());
                            }
                        }
                        _ => {}
                    }
                    node.metadata = Arc::new(element);
                }

                let is_top = matches!(
                    node.kind,
                    NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation
                );

                unit.add_node(node.clone());
                if is_top {
                    unit.add_edge(
                        container_id.clone().into(),
                        node.id.clone(),
                        GraphEdge::new(EdgeType::Contains),
                    );
                }
            }

            for rel in &parse_result.output.relations {
                let mut resolved_target_str = rel.target_id.to_string();

                if let (Some(tree), Some(source)) = (&parse_result.tree, &parse_result.source) {
                    if let Some(r) = &rel.range {
                        let point = tree_sitter::Point::new(r.start_line, r.start_col);
                        if let Some(node) = tree
                            .root_node()
                            .named_descendant_for_point_range(point, point)
                        {
                            let context = ResolutionContext::new_with_unit(
                                node,
                                rel.target_id.to_string(),
                                &dummy_index,
                                Some(&unit),
                                source,
                                tree,
                                &self.parser,
                            );

                            if let Some(SymbolResolution::Precise(fqn, _)) =
                                self.resolve_symbol_internal(&context)
                            {
                                resolved_target_str = fqn;
                            } else {
                                // Fallback
                                if !resolved_target_str.contains('.') {
                                    if let Some(res) = type_provider
                                        .resolve_type_name(&resolved_target_str, &res_ctx)
                                    {
                                        resolved_target_str = res;
                                    }
                                }
                            }
                        }
                    }
                }

                let edge = GraphEdge::new(rel.edge_type.clone());

                if resolved_target_str == rel.target_id.to_string()
                    && matches!(rel.target_id, NodeId::Structured(_))
                {
                    unit.add_edge(rel.source_id.clone(), rel.target_id.clone(), edge);
                    continue;
                }

                let segments: Vec<&str> = resolved_target_str
                    .split(|c| c == '.' || c == '#')
                    .collect();
                let mut structured_parts: Vec<(NodeKind, String)> = Vec::new();

                for (i, part) in segments.iter().enumerate() {
                    let mut found_kind = NodeKind::Package;
                    let is_last = i == segments.len() - 1;

                    let candidates = [
                        NodeKind::Class,
                        NodeKind::Interface,
                        NodeKind::Enum,
                        NodeKind::Annotation,
                        NodeKind::Method,
                        NodeKind::Field,
                        NodeKind::Constructor,
                    ];

                    let mut matched = false;
                    for k in &candidates {
                        let mut probe_parts = structured_parts.clone();
                        probe_parts.push((k.clone(), part.to_string()));
                        let id = NodeId::Structured(probe_parts);
                        if unit.nodes.contains_key(&id) {
                            found_kind = k.clone();
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        if is_last {
                            if rel.edge_type == EdgeType::Implements
                                || rel.edge_type == EdgeType::InheritsFrom
                                || rel.edge_type == EdgeType::TypedAs
                                || rel.edge_type == EdgeType::DecoratedBy
                            {
                                found_kind = NodeKind::Class;
                            } else if part.chars().next().map_or(false, |c| c.is_uppercase()) {
                                found_kind = NodeKind::Class;
                            }
                        } else if part.chars().next().map_or(false, |c| c.is_uppercase()) {
                            found_kind = NodeKind::Class;
                        } else {
                            found_kind = NodeKind::Package;
                        }
                    }

                    structured_parts.push((found_kind, part.to_string()));
                }

                let final_target_id = NodeId::Structured(structured_parts);

                unit.add_edge(rel.source_id.clone(), final_target_id, edge);
            }
        }

        Ok(unit)
    }
}
