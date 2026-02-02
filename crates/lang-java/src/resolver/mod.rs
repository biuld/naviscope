use crate::model::{JavaIndexMetadata, JavaNodeMetadata};
use crate::parser::JavaParser;
use naviscope_api::models::symbol::matches_intent;
use naviscope_api::models::{SymbolIntent, SymbolResolution, TypeRef};
use naviscope_core::error::Result;
use naviscope_core::features::CodeGraphLike;
use naviscope_core::ingest::resolver::{LangResolver, ProjectContext, SemanticResolver};
use naviscope_core::ingest::scanner::{ParsedContent, ParsedFile};
use naviscope_core::model::CodeGraph;
use naviscope_core::model::{EdgeType, GraphEdge, GraphOp, NodeKind, ResolvedUnit};
use petgraph::stable_graph::NodeIndex;
use std::ops::ControlFlow;
use std::sync::Arc;
use tree_sitter::Tree;

pub mod context;
pub mod scope;

use context::ResolutionContext;
use scope::{BuiltinScope, ImportScope, LocalScope, MemberScope, Scope};

#[derive(Clone)]
pub struct JavaResolver {
    pub parser: JavaParser,
}

impl JavaResolver {
    pub fn new() -> Self {
        Self {
            parser: JavaParser::new().expect("Failed to initialize JavaParser"),
        }
    }

    fn get_active_scopes<'a>(&'a self, ctx: &'a ResolutionContext) -> Vec<Box<dyn Scope + 'a>> {
        let mut scopes: Vec<Box<dyn Scope + 'a>> = Vec::new();

        if ctx.receiver_node.is_none() {
            scopes.push(Box::new(LocalScope {
                parser: &self.parser,
            }));
        }

        scopes.push(Box::new(MemberScope {
            parser: &self.parser,
        }));
        scopes.push(Box::new(ImportScope {
            parser: &self.parser,
        }));

        if ctx.intent == SymbolIntent::Type {
            scopes.push(Box::new(BuiltinScope {
                parser: &self.parser,
            }));
        }

        scopes
    }

    fn resolve_type_ref(
        &self,
        type_ref: &TypeRef,
        package: Option<&str>,
        imports: &[String],
        known_fqns: &std::collections::HashSet<String>,
    ) -> TypeRef {
        match type_ref {
            TypeRef::Raw(name) => {
                // 1. Check if name matches a known FQN suffix in the same file (Inner class priority)
                if let Some(fqn) = known_fqns
                    .iter()
                    .find(|k| k.ends_with(&format!(".{}", name)) || *k == name)
                {
                    // Simple heuristic: if the name matches the end of a known FQN, use it.
                    // This handles 'Source' -> '...DefaultApplicationArguments.Source'
                    return TypeRef::Id(fqn.clone());
                }

                if let Some(fqn) = self
                    .parser
                    .resolve_type_name_to_fqn_data(name, package, imports)
                {
                    TypeRef::Id(fqn)
                } else {
                    TypeRef::Raw(name.clone())
                }
            }
            TypeRef::Generic { base, args } => TypeRef::Generic {
                base: Box::new(self.resolve_type_ref(base, package, imports, known_fqns)),
                args: args
                    .iter()
                    .map(|a| self.resolve_type_ref(a, package, imports, known_fqns))
                    .collect(),
            },
            TypeRef::Array {
                element,
                dimensions,
            } => TypeRef::Array {
                element: Box::new(self.resolve_type_ref(element, package, imports, known_fqns)),
                dimensions: *dimensions,
            },
            TypeRef::Wildcard {
                bound,
                is_upper_bound,
            } => TypeRef::Wildcard {
                bound: bound
                    .as_ref()
                    .map(|b| Box::new(self.resolve_type_ref(b, package, imports, known_fqns))),
                is_upper_bound: *is_upper_bound,
            },
            _ => type_ref.clone(),
        }
    }

    pub fn resolve_symbol_internal(&self, context: &ResolutionContext) -> Option<SymbolResolution> {
        match self.get_active_scopes(context).into_iter().try_fold(
            None,
            |_: Option<SymbolResolution>, scope: Box<dyn Scope>| match scope
                .resolve(&context.name, context)
            {
                Some(Ok(res)) => ControlFlow::Break(Some(res)),
                Some(Err(())) => ControlFlow::Break(None),
                None => ControlFlow::Continue(None),
            },
        ) {
            ControlFlow::Break(res) => res,
            ControlFlow::Continue(_) => None,
        }
    }
}

impl SemanticResolver for JavaResolver {
    fn resolve_at(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        byte_col: usize,
        index: &dyn CodeGraphLike,
    ) -> Option<SymbolResolution> {
        let point = tree_sitter::Point::new(line, byte_col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
            .filter(|n| {
                matches!(
                    n.kind(),
                    "identifier" | "type_identifier" | "scoped_identifier" | "this"
                )
            })?;

        let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
        let context = ResolutionContext::new(node, name, index, source, tree, &self.parser);

        self.resolve_symbol_internal(&context)
    }

    fn find_matches(
        &self,
        index: &dyn CodeGraphLike,
        resolution: &SymbolResolution,
    ) -> Vec<NodeIndex> {
        match resolution {
            SymbolResolution::Local(_, _) => vec![],
            SymbolResolution::Precise(fqn, intent) => {
                let fids = index.fqns().resolve_fqn_string(fqn);
                eprintln!(
                    "DEBUG: find_matches Precise '{}' intent={:?} -> fids={:?}",
                    fqn, intent, fids
                );
                for fid in fids {
                    if let Some(&idx) = index.fqn_map().get(&fid) {
                        if let Some(node) = index.topology().node_weight(idx) {
                            eprintln!("DEBUG: Candidate node idx={:?} kind={:?}", idx, node.kind);
                            if *intent == SymbolIntent::Unknown
                                || matches_intent(&node.kind, *intent)
                            {
                                return vec![idx];
                            }
                        }
                    }
                }
                vec![]
            }
            SymbolResolution::Global(fqn) => {
                let fids = index.fqns().resolve_fqn_string(fqn);
                eprintln!("DEBUG: find_matches Global '{}' -> fids={:?}", fqn, fids);
                for fid in fids {
                    if let Some(&idx) = index.fqn_map().get(&fid) {
                        return vec![idx];
                    }
                }
                vec![]
            }
        }
    }

    fn resolve_type_of(
        &self,
        index: &dyn CodeGraphLike,
        resolution: &SymbolResolution,
    ) -> Vec<SymbolResolution> {
        // Reuse original logic
        let mut type_resolutions = Vec::new();

        match resolution {
            SymbolResolution::Local(_, type_name) => {
                if let Some(tn) = type_name {
                    if let Some(fqn) = self.parser.resolve_type_name_to_fqn_data(tn, None, &[]) {
                        type_resolutions.push(SymbolResolution::Precise(fqn, SymbolIntent::Type));
                    }
                }
            }
            SymbolResolution::Precise(fqn, intent) => {
                let fids = index.fqns().resolve_fqn_string(fqn);
                for fid in fids {
                    if let Some(&idx) = index.fqn_map().get(&fid) {
                        let node = &index.topology()[idx];
                        if let Some(java_meta) =
                            node.metadata.as_any().downcast_ref::<JavaNodeMetadata>()
                        {
                            match java_meta {
                                JavaNodeMetadata::Field { type_ref, .. } => match type_ref {
                                    TypeRef::Raw(s) => type_resolutions.push(
                                        SymbolResolution::Precise(s.clone(), SymbolIntent::Type),
                                    ),
                                    TypeRef::Id(id) => type_resolutions.push(
                                        SymbolResolution::Precise(id.clone(), SymbolIntent::Type),
                                    ),
                                    _ => {}
                                },
                                JavaNodeMetadata::Method { return_type, .. } => match return_type {
                                    TypeRef::Raw(s) => type_resolutions.push(
                                        SymbolResolution::Precise(s.clone(), SymbolIntent::Type),
                                    ),
                                    TypeRef::Id(id) => type_resolutions.push(
                                        SymbolResolution::Precise(id.clone(), SymbolIntent::Type),
                                    ),
                                    _ => {}
                                },
                                _ => {
                                    if matches_intent(&node.kind, SymbolIntent::Type) {
                                        type_resolutions.push(resolution.clone());
                                    }
                                }
                            }
                        }
                    }
                }
                if type_resolutions.is_empty() && *intent == SymbolIntent::Type {
                    type_resolutions.push(resolution.clone());
                }
            }
            SymbolResolution::Global(fqn) => {
                let fids = index.fqns().resolve_fqn_string(fqn);
                for fid in fids {
                    if let Some(&idx) = index.fqn_map().get(&fid) {
                        let node = &index.topology()[idx];
                        if matches_intent(&node.kind, SymbolIntent::Type) {
                            type_resolutions.push(resolution.clone());
                        }
                    }
                }
            }
        }
        type_resolutions
    }

    fn find_implementations(
        &self,
        index: &dyn CodeGraphLike,
        resolution: &SymbolResolution,
    ) -> Vec<NodeIndex> {
        let target_nodes = self.find_matches(index, resolution);
        let mut results = Vec::new();

        for &node_idx in &target_nodes {
            let node = &index.topology()[node_idx];

            // Check if it's a method
            let is_method = if let Some(java_meta) =
                node.metadata.as_any().downcast_ref::<JavaNodeMetadata>()
            {
                matches!(java_meta, JavaNodeMetadata::Method { .. })
            } else {
                false
            };

            if is_method {
                // 1. Find the enclosing class/interface
                let mut parent_incoming = index
                    .topology()
                    .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                    .detach();
                while let Some((edge_idx, parent_idx)) = parent_incoming.next(index.topology()) {
                    if index.topology()[edge_idx].edge_type == EdgeType::Contains {
                        // 2. Find all implementations of this parent
                        use naviscope_plugin::NamingConvention;
                        let parent_fqn = crate::naming::JavaNamingConvention.render_fqn(
                            index.topology()[parent_idx].id,
                            index.fqns()
                        );
                        let parent_res = SymbolResolution::Precise(parent_fqn, SymbolIntent::Type);
                        let impl_classes = self.find_implementations(index, &parent_res);

                        // 3. For each impl class, find a method with same name
                        for impl_class_idx in impl_classes {
                            let mut children = index
                                .topology()
                                .neighbors_directed(impl_class_idx, petgraph::Direction::Outgoing)
                                .detach();
                            while let Some((c_edge_idx, child_idx)) =
                                children.next(index.topology())
                            {
                                if index.topology()[c_edge_idx].edge_type == EdgeType::Contains {
                                    let child_node = &index.topology()[child_idx];
                                    let is_child_method = if let Some(java_meta) = child_node
                                        .metadata
                                        .as_any()
                                        .downcast_ref::<JavaNodeMetadata>()
                                    {
                                        matches!(java_meta, JavaNodeMetadata::Method { .. })
                                    } else {
                                        false
                                    };

                                    if is_child_method && child_node.name == node.name {
                                        results.push(child_idx);
                                    }
                                }
                            }
                        }
                    }
                }
                continue;
            }

            let mut incoming = index
                .topology()
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(index.topology()) {
                let edge = &index.topology()[edge_idx];
                if edge.edge_type == EdgeType::Implements
                    || edge.edge_type == EdgeType::InheritsFrom
                {
                    results.push(neighbor_idx);
                }
            }
        }
        results
    }
}

impl LangResolver for JavaResolver {
    fn resolve(&self, file: &ParsedFile, context: &ProjectContext) -> Result<ResolvedUnit> {
        let mut unit = ResolvedUnit::new();
        let dummy_index = CodeGraph::empty();

        let parse_result_owned;
        let parse_result = match &file.content {
            ParsedContent::Language(res) => res,
            ParsedContent::Unparsed(src) => {
                if file.path().extension().map_or(false, |e| e == "java") {
                    use naviscope_core::ingest::parser::IndexParser;
                    parse_result_owned = self.parser.parse_file(src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Ok(unit);
                }
            }
            ParsedContent::Lazy => {
                if file.path().extension().map_or(false, |e| e == "java") {
                    let src = file.read_content().map_err(|e| {
                        naviscope_core::error::NaviscopeError::Internal(format!(
                            "Failed to read file {}: {}",
                            file.path().display(),
                            e
                        ))
                    })?;
                    use naviscope_core::ingest::parser::IndexParser;
                    parse_result_owned = self.parser.parse_file(&src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Ok(unit);
                }
            }
            _ => return Ok(unit),
        };

        {
            // Scope for usage of parse_result
            unit.identifiers = parse_result.output.identifiers.clone();
            unit.ops.push(GraphOp::UpdateIdentifiers {
                path: Arc::from(file.file.path.as_path()),
                identifiers: unit.identifiers.clone(),
            });

            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            let container_id = if let Some(pkg_name) = &parse_result.package_name {
                let package_id = pkg_name.to_string();

                let package_node = naviscope_core::ingest::parser::IndexNode {
                    id: package_id.clone().into(),
                    name: pkg_name.to_string(),
                    kind: NodeKind::Package,
                    lang: "java".to_string(),
                    location: None,
                    metadata: Arc::new(crate::model::JavaIndexMetadata::Package),
                };

                unit.add_node(package_node);

                unit.add_edge(
                    module_id.clone().into(),
                    package_id.clone().into(),
                    GraphEdge::new(EdgeType::Contains),
                );

                package_id
            } else {
                // For default package, we might want to use a semantic "default package" node
                // or just attach to module.
                // For now, attaching to module seems safer to avoid colliding all default packages.
                // But this means default package classes might be harder to find via clean FQN if module_id is weird.
                module_id
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

            for node in &parse_result.output.nodes {
                let mut node = node.clone();

                if let Some(java_idx_meta) = node
                    .metadata
                    .as_any()
                    .downcast_ref::<crate::model::JavaIndexMetadata>()
                {
                    let mut element = java_idx_meta.clone();

                    match &mut element {
                        JavaIndexMetadata::Method {
                            return_type,
                            parameters,
                            ..
                        } => {
                            *return_type = self.resolve_type_ref(
                                return_type,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_types,
                            );
                            for param in parameters {
                                param.type_ref = self.resolve_type_ref(
                                    &param.type_ref,
                                    parse_result.package_name.as_deref(),
                                    &parse_result.imports,
                                    &known_types,
                                );
                                if let TypeRef::Id(type_fqn) = &param.type_ref {
                                    local_type_map.insert(node.name.clone(), type_fqn.clone());
                                }
                            }
                        }
                        JavaIndexMetadata::Field { type_ref, .. } => {
                            *type_ref = self.resolve_type_ref(
                                type_ref,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_types,
                            );
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
                                // ... Fallbacks ...
                                if !resolved_target_str.contains('.') {
                                    if let Some(res) = self.parser.resolve_type_name_to_fqn_data(
                                        &resolved_target_str,
                                        parse_result.package_name.as_deref(),
                                        &parse_result.imports,
                                    ) {
                                        resolved_target_str = res;
                                    }
                                }
                            }
                        }
                    }
                }

                let edge = GraphEdge::new(rel.edge_type.clone());

                // Optimization: If the resolved string matches the original target ID string,
                // trust the original ID IF it is Structured (which preserves metadata from parser).
                if resolved_target_str == rel.target_id.to_string()
                    && matches!(
                        rel.target_id,
                        naviscope_api::models::symbol::NodeId::Structured(_)
                    )
                {
                    unit.add_edge(rel.source_id.clone(), rel.target_id.clone(), edge);
                    continue;
                }

                // Try to reconstruct a Structured ID to match the graph nodes
                let segments: Vec<&str> = resolved_target_str
                    .split(|c| c == '.' || c == '#')
                    .collect();
                let mut structured_parts: Vec<(naviscope_api::models::graph::NodeKind, String)> =
                    Vec::new();

                for (i, part) in segments.iter().enumerate() {
                    let mut found_kind = naviscope_api::models::graph::NodeKind::Package;
                    let is_last = i == segments.len() - 1;

                    // Probe kinds in unit.nodes
                    let candidates = [
                        naviscope_api::models::graph::NodeKind::Class,
                        naviscope_api::models::graph::NodeKind::Interface,
                        naviscope_api::models::graph::NodeKind::Enum,
                        naviscope_api::models::graph::NodeKind::Annotation,
                        naviscope_api::models::graph::NodeKind::Method,
                        naviscope_api::models::graph::NodeKind::Field,
                        naviscope_api::models::graph::NodeKind::Constructor,
                    ];

                    let mut matched = false;
                    for k in &candidates {
                        let mut probe_parts = structured_parts.clone();
                        probe_parts.push((k.clone(), part.to_string()));
                        let id = naviscope_api::models::symbol::NodeId::Structured(probe_parts);
                        if unit.nodes.contains_key(&id) {
                            found_kind = k.clone();
                            matched = true;
                            break;
                        }
                    }

                    if !matched {
                        if is_last {
                            // Heuristics for last part if not found
                            if rel.edge_type == EdgeType::Implements {
                                found_kind = naviscope_api::models::graph::NodeKind::Interface;
                            } else if rel.edge_type == EdgeType::DecoratedBy {
                                found_kind = naviscope_api::models::graph::NodeKind::Annotation;
                            } else if rel.edge_type == EdgeType::InheritsFrom {
                                if let Some(src_node) = unit.nodes.get(&rel.source_id) {
                                    match src_node.kind {
                                        naviscope_api::models::graph::NodeKind::Interface => {
                                            found_kind =
                                                naviscope_api::models::graph::NodeKind::Interface
                                        }
                                        _ => {
                                            found_kind =
                                                naviscope_api::models::graph::NodeKind::Class
                                        }
                                    }
                                } else {
                                    found_kind = naviscope_api::models::graph::NodeKind::Class;
                                }
                            } else if rel.edge_type == EdgeType::TypedAs {
                                found_kind = naviscope_api::models::graph::NodeKind::Class;
                            } else if part.chars().next().map_or(false, |c| c.is_uppercase()) {
                                found_kind = naviscope_api::models::graph::NodeKind::Class;
                            }
                        } else {
                            found_kind = naviscope_api::models::graph::NodeKind::Package;
                        }
                    }

                    structured_parts.push((found_kind, part.to_string()));
                }

                let final_target_id =
                    naviscope_api::models::symbol::NodeId::Structured(structured_parts);
                unit.add_edge(rel.source_id.clone(), final_target_id, edge);
            }
        }

        Ok(unit)
    }
}
