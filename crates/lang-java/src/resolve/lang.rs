use crate::JavaPlugin;
use crate::inference::adapters::HeuristicAdapter;
use crate::inference::{TypeProvider, TypeResolutionContext};
use crate::model::JavaIndexMetadata;
use crate::resolve::context::ResolutionContext;
use naviscope_api::models::graph::{EdgeType, GraphEdge, NodeKind};
use naviscope_api::models::symbol::{NodeId, SymbolResolution};
use naviscope_plugin::{
    GlobalParseResult, GraphOp, IndexNode, IndexRelation, ParsedContent, ParsedFile, ProjectContext,
    ResolvedUnit, SourceAnalyzeArtifact, SourceCollectArtifact, SourceIndexCap,
};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

struct CollectOutput {
    unit: ResolvedUnit,
    container_id: NodeId,
}

struct AnalyzeOutput {
    unit: ResolvedUnit,
    res_ctx: TypeResolutionContext,
    bound_relations: Vec<BoundRelation>,
    deferred_relations: Vec<DeferredRelation>,
}

struct BoundRelation {
    source_id: NodeId,
    target_id: NodeId,
    edge: GraphEdge,
}

struct DeferredRelation {
    raw_target: String,
}

struct JavaCollectArtifact {
    parse_result: GlobalParseResult,
    collected: CollectOutput,
    type_symbols: Vec<String>,
    method_symbols: Vec<String>,
    provided_dependency_symbols: Vec<String>,
    required_dependency_symbols: Vec<String>,
}

struct JavaAnalyzeArtifact {
    parse_result: GlobalParseResult,
    analyzed: AnalyzeOutput,
}

impl SourceCollectArtifact for JavaCollectArtifact {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any + Send + Sync> {
        self
    }

    fn collected_type_symbols(&self) -> &[String] {
        &self.type_symbols
    }

    fn collected_method_symbols(&self) -> &[String] {
        &self.method_symbols
    }

    fn provided_dependency_symbols(&self) -> &[String] {
        &self.provided_dependency_symbols
    }

    fn required_dependency_symbols(&self) -> &[String] {
        &self.required_dependency_symbols
    }
}

impl SourceAnalyzeArtifact for JavaAnalyzeArtifact {
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any + Send + Sync> {
        self
    }
}

impl SourceIndexCap for JavaPlugin {
    fn collect_source(
        &self,
        file: &ParsedFile,
        context: &ProjectContext,
    ) -> std::result::Result<Box<dyn SourceCollectArtifact>, Box<dyn std::error::Error + Send + Sync>>
    {
        let parse_result_owned;
        let parse_result = match &file.content {
            ParsedContent::Language(res) => res,
            ParsedContent::Unparsed(src) => {
                if file.path().extension().is_some_and(|e| e == "java") {
                    parse_result_owned = self.parser.parse_file(src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Err("Unsupported non-java file in Java collect_source".into());
                }
            }
            ParsedContent::Lazy => {
                if file.path().extension().is_some_and(|e| e == "java") {
                    let src = std::fs::read_to_string(file.path()).map_err(|e| {
                        format!("Failed to read file {}: {}", file.path().display(), e)
                    })?;
                    parse_result_owned = self.parser.parse_file(&src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Err("Unsupported non-java file in Java collect_source".into());
                }
            }
            _ => return Err("Unsupported parsed content in Java collect_source".into()),
        };

        let collected = self.collect_pass(file, context, parse_result);
        let type_symbols: Vec<String> = parse_result
            .output
            .nodes
            .iter()
            .filter(|node| {
                matches!(
                    node.kind,
                    NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation
                )
            })
            .map(|node| node.id.to_string())
            .collect();
        let method_symbols: Vec<String> = parse_result
            .output
            .nodes
            .iter()
            .filter(|node| matches!(node.kind, NodeKind::Method | NodeKind::Constructor))
            .map(|node| node.id.to_string())
            .collect();
        let mut provided_dependency_symbols = type_symbols.clone();
        if let Some(pkg) = &parse_result.package_name {
            provided_dependency_symbols.push(format!("package:{pkg}"));
        }
        let mut required_dependency_symbols = Vec::new();
        if let Some(pkg) = &parse_result.package_name {
            required_dependency_symbols.push(format!("package:{pkg}"));
        }
        for import in &parse_result.imports {
            if let Some(pkg) = import.strip_suffix(".*") {
                required_dependency_symbols.push(format!("package:{pkg}"));
            } else {
                required_dependency_symbols.push(import.clone());
            }
        }

        Ok(Box::new(JavaCollectArtifact {
            parse_result: parse_result.clone(),
            collected,
            type_symbols,
            method_symbols,
            provided_dependency_symbols,
            required_dependency_symbols,
        }))
    }

    fn analyze_source(
        &self,
        collected: Box<dyn SourceCollectArtifact>,
        context: &ProjectContext,
    ) -> std::result::Result<Box<dyn SourceAnalyzeArtifact>, Box<dyn std::error::Error + Send + Sync>>
    {
        let collected = collected
            .into_any()
            .downcast::<JavaCollectArtifact>()
            .map_err(|_| "Java analyze_source received incompatible collect artifact")?;
        let mut analyzed = self.analyze_pass(collected.collected, &collected.parse_result, context);
        self.bind_all_relations(&mut analyzed, &collected.parse_result);

        Ok(Box::new(JavaAnalyzeArtifact {
            parse_result: collected.parse_result,
            analyzed,
        }))
    }

    fn lower_source(
        &self,
        analyzed: Box<dyn SourceAnalyzeArtifact>,
        _context: &ProjectContext,
    ) -> std::result::Result<ResolvedUnit, Box<dyn std::error::Error + Send + Sync>> {
        let analyzed = analyzed
            .into_any()
            .downcast::<JavaAnalyzeArtifact>()
            .map_err(|_| "Java lower_source received incompatible analyze artifact")?;
        self.lower_pass(analyzed.analyzed, &analyzed.parse_result)
    }
}

impl JavaPlugin {
    fn collect_pass(
        &self,
        file: &ParsedFile,
        context: &ProjectContext,
        parse_result: &GlobalParseResult,
    ) -> CollectOutput {
        let mut unit = ResolvedUnit::new();
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
                .map(|s| (NodeKind::Package, s.to_string()))
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

        CollectOutput {
            unit,
            container_id,
        }
    }

    fn analyze_pass(
        &self,
        collected: CollectOutput,
        parse_result: &GlobalParseResult,
        context: &ProjectContext,
    ) -> AnalyzeOutput {
        let mut known_types = HashSet::<String>::new();
        known_types.extend(context.symbol_table.type_symbols.iter().cloned());
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
            known_fqns: known_types.into_iter().collect(),
        };

        let type_provider = HeuristicAdapter;
        let mut unit = collected.unit;

        for node in &parse_result.output.nodes {
            let mut node = node.clone();

            if let Some(java_idx_meta) = node.metadata.as_any().downcast_ref::<JavaIndexMetadata>() {
                let mut element = java_idx_meta.clone();
                match &mut element {
                    JavaIndexMetadata::Method {
                        return_type,
                        parameters,
                        ..
                    } => {
                        *return_type = self.resolve_type_ref(return_type, &type_provider, &res_ctx);
                        for param in parameters {
                            param.type_ref = self.resolve_type_ref(
                                &param.type_ref,
                                &type_provider,
                                &res_ctx,
                            );
                        }
                    }
                    JavaIndexMetadata::Field { type_ref, .. } => {
                        *type_ref = self.resolve_type_ref(type_ref, &type_provider, &res_ctx);
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
                    collected.container_id.clone().into(),
                    node.id.clone(),
                    GraphEdge::new(EdgeType::Contains),
                );
            }
        }

        AnalyzeOutput {
            unit,
            res_ctx,
            bound_relations: Vec::new(),
            deferred_relations: Vec::new(),
        }
    }

    fn bind_all_relations(
        &self,
        analyzed: &mut AnalyzeOutput,
        parse_result: &GlobalParseResult,
    ) {
        for rel in &parse_result.output.relations {
            self.bind_relation(rel, parse_result, analyzed);
        }
    }

    fn lower_pass(
        &self,
        mut analyzed: AnalyzeOutput,
        _parse_result: &GlobalParseResult,
    ) -> std::result::Result<ResolvedUnit, Box<dyn std::error::Error + Send + Sync>> {
        for bound in analyzed.bound_relations.drain(..) {
            analyzed
                .unit
                .add_edge(bound.source_id, bound.target_id, bound.edge);
        }

        for deferred in analyzed.deferred_relations.drain(..) {
            analyzed.unit.deferred_symbols.push(naviscope_plugin::DeferredSymbol {
                target: deferred.raw_target,
            });
        }

        Ok(analyzed.unit)
    }

    fn bind_relation(
        &self,
        rel: &IndexRelation,
        parse_result: &GlobalParseResult,
        analyzed: &mut AnalyzeOutput,
    ) {
        let dummy_index = naviscope_plugin::EmptyCodeGraph;
        let type_provider = HeuristicAdapter;

        let original_target = rel.target_id.to_string();
        let mut resolved_target = original_target.clone();
        let mut precise_bound = false;

        if let (Some(tree), Some(source), Some(r)) = (&parse_result.tree, &parse_result.source, &rel.range)
        {
            let point = tree_sitter::Point::new(r.start_line, r.start_col);
            if let Some(node) = tree
                .root_node()
                .named_descendant_for_point_range(point, point)
            {
                let context = ResolutionContext::new_with_unit(
                    node,
                    original_target.clone(),
                    &dummy_index,
                    Some(&analyzed.unit),
                    source,
                    tree,
                    &self.parser,
                );

                if let Some(SymbolResolution::Precise(fqn, _)) = self.resolve_symbol_internal(&context) {
                    resolved_target = fqn;
                    precise_bound = true;
                } else if !resolved_target.contains('.') {
                    if let Some(res) =
                        type_provider.resolve_type_name(&resolved_target, &analyzed.res_ctx)
                    {
                        resolved_target = res;
                    }
                }
            }
        }

        if !precise_bound && !matches!(rel.target_id, NodeId::Structured(_)) {
            analyzed.deferred_relations.push(DeferredRelation {
                raw_target: original_target.clone(),
            });
        }

        let target_id = if resolved_target == original_target && matches!(rel.target_id, NodeId::Structured(_))
        {
            rel.target_id.clone()
        } else {
            Self::build_target_node_id(
                &resolved_target,
                &rel.edge_type,
                &analyzed.unit.nodes,
            )
        };

        analyzed.bound_relations.push(BoundRelation {
            source_id: rel.source_id.clone(),
            target_id,
            edge: GraphEdge::new(rel.edge_type.clone()),
        });
    }

    fn build_target_node_id(
        target: &str,
        edge_type: &EdgeType,
        known_nodes: &HashMap<NodeId, IndexNode>,
    ) -> NodeId {
        let segments: Vec<&str> = target.split(['.', '#']).collect();
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
                if known_nodes.contains_key(&id) {
                    found_kind = k.clone();
                    matched = true;
                    break;
                }
            }

            if !matched {
                if is_last {
                    if *edge_type == EdgeType::Implements
                        || *edge_type == EdgeType::InheritsFrom
                        || *edge_type == EdgeType::TypedAs
                        || *edge_type == EdgeType::DecoratedBy
                    {
                        found_kind = NodeKind::Class;
                    } else if part.chars().next().is_some_and(|c| c.is_uppercase()) {
                        found_kind = NodeKind::Class;
                    }
                } else if part.chars().next().is_some_and(|c| c.is_uppercase()) {
                    found_kind = NodeKind::Class;
                } else {
                    found_kind = NodeKind::Package;
                }
            }

            structured_parts.push((found_kind, part.to_string()));
        }

        NodeId::Structured(structured_parts)
    }
}
