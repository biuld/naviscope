use crate::model::{JavaElement, JavaPackage};
use crate::parser::JavaParser;
use naviscope_api::models::TypeRef;
use naviscope_core::engine::CodeGraph;
use naviscope_core::engine::storage::GLOBAL_POOL;
use naviscope_core::error::Result;
use naviscope_core::model::{EdgeType, GraphEdge, GraphNode, GraphOp, NodeKind, ResolvedUnit};
use naviscope_core::parser::SymbolIntent;
use naviscope_core::parser::{SymbolResolution, matches_intent};
use naviscope_core::project::scanner::{ParsedContent, ParsedFile};
use naviscope_core::query::CodeGraphLike;
use naviscope_core::resolver::SemanticResolver;
use naviscope_core::resolver::{LangResolver, ProjectContext};
use petgraph::stable_graph::NodeIndex;
use smol_str::SmolStr;
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

    fn is_top_level_node(&self, node: &GraphNode) -> bool {
        let kind = node.kind();
        matches!(
            kind,
            NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation
        )
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
                if let Some(&idx) = index.fqn_map().get(fqn.as_str()) {
                    if let Some(node) = index.topology().node_weight(idx) {
                        if *intent == SymbolIntent::Unknown || matches_intent(&node.kind(), *intent)
                        {
                            return vec![idx];
                        }
                    }
                }
                vec![]
            }
            SymbolResolution::Global(fqn) => {
                if let Some(&idx) = index.fqn_map().get(fqn.as_str()) {
                    return vec![idx];
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
                if let Some(&idx) = index.fqn_map().get(fqn.as_str()) {
                    let node = &index.topology()[idx];
                    if let Ok(element) =
                        serde_json::from_value::<JavaElement>(node.metadata.clone())
                    {
                        match element {
                            JavaElement::Field(f) => match &f.type_ref {
                                TypeRef::Raw(s) => type_resolutions
                                    .push(SymbolResolution::Precise(s.clone(), SymbolIntent::Type)),
                                TypeRef::Id(id) => type_resolutions.push(
                                    SymbolResolution::Precise(id.clone(), SymbolIntent::Type),
                                ),
                                _ => {}
                            },
                            JavaElement::Method(m) => match &m.return_type {
                                TypeRef::Raw(s) => type_resolutions
                                    .push(SymbolResolution::Precise(s.clone(), SymbolIntent::Type)),
                                TypeRef::Id(id) => type_resolutions.push(
                                    SymbolResolution::Precise(id.clone(), SymbolIntent::Type),
                                ),
                                _ => {}
                            },
                            _ => {
                                if matches_intent(&node.kind(), SymbolIntent::Type) {
                                    type_resolutions.push(resolution.clone());
                                }
                            }
                        }
                    }
                } else if *intent == SymbolIntent::Type {
                    type_resolutions.push(resolution.clone());
                }
            }
            SymbolResolution::Global(fqn) => {
                if let Some(&idx) = index.fqn_map().get(fqn.as_str()) {
                    let node = &index.topology()[idx];
                    if matches_intent(&node.kind(), SymbolIntent::Type) {
                        type_resolutions.push(resolution.clone());
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
            if let Ok(element) = serde_json::from_value::<JavaElement>(node.metadata.clone()) {
                if let JavaElement::Method(_m) = element {
                    // 1. Find the enclosing class/interface
                    let mut parent_incoming = index
                        .topology()
                        .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                        .detach();
                    while let Some((edge_idx, parent_idx)) = parent_incoming.next(index.topology())
                    {
                        if index.topology()[edge_idx].edge_type == EdgeType::Contains {
                            // 2. Find all implementations of this parent
                            let parent_fqn = index.topology()[parent_idx].fqn().to_string();
                            let parent_res =
                                SymbolResolution::Precise(parent_fqn, SymbolIntent::Type);
                            let impl_classes = self.find_implementations(index, &parent_res);

                            // 3. For each impl class, find a method with same name
                            for impl_class_idx in impl_classes {
                                let mut children = index
                                    .topology()
                                    .neighbors_directed(
                                        impl_class_idx,
                                        petgraph::Direction::Outgoing,
                                    )
                                    .detach();
                                while let Some((c_edge_idx, child_idx)) =
                                    children.next(index.topology())
                                {
                                    if index.topology()[c_edge_idx].edge_type == EdgeType::Contains
                                    {
                                        if let Ok(child_element) =
                                            serde_json::from_value::<JavaElement>(
                                                index.topology()[child_idx].metadata.clone(),
                                            )
                                        {
                                            if let JavaElement::Method(_) = child_element {
                                                if index.topology()[child_idx].name == node.name {
                                                    results.push(child_idx);
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    continue;
                }
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
                    use naviscope_core::parser::IndexParser;
                    parse_result_owned = self.parser.parse_file(src, Some(&file.file.path))?;
                    &parse_result_owned
                } else {
                    return Ok(unit);
                }
            }
            _ => return Ok(unit),
        };

        {
            // Scope for usage of parse_result
            unit.identifiers = parse_result
                .identifiers
                .iter()
                .map(|s| SmolStr::from(s))
                .collect();
            unit.ops.push(GraphOp::UpdateIdentifiers {
                path: GLOBAL_POOL.intern_path(&file.file.path),
                identifiers: unit.identifiers.clone(),
            });

            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            let container_id = if let Some(pkg_name) = &parse_result.package_name {
                let package_id = if module_id.contains("::") {
                    format!("{}.{}", module_id, pkg_name)
                } else {
                    format!("{}::{}", module_id, pkg_name)
                };

                let package_node = GraphNode {
                    id: Arc::from(package_id.as_str()),
                    name: SmolStr::from(pkg_name.as_str()),
                    kind: NodeKind::Package,
                    lang: Arc::from("java"),
                    location: None,
                    metadata: serde_json::to_value(JavaElement::Package(JavaPackage {}))
                        .unwrap_or(serde_json::Value::Null),
                };

                unit.add_node(Arc::from(package_id.as_str()), package_node);

                unit.add_edge(
                    Arc::from(module_id.as_str()),
                    Arc::from(package_id.as_str()),
                    GraphEdge::new(EdgeType::Contains),
                );

                package_id
            } else {
                module_id
            };

            let mut known_types = std::collections::HashSet::<String>::new();
            let mut local_type_map = std::collections::HashMap::<String, String>::new();
            let _dummy_index = CodeGraph::empty();

            for node in &parse_result.nodes {
                if self.is_top_level_node(node) {
                    known_types.insert(node.fqn().to_string());
                }
            }

            for node in &parse_result.nodes {
                let fqn = node.fqn();
                let mut node = node.clone();

                if let Ok(mut element) =
                    serde_json::from_value::<JavaElement>(node.metadata.clone())
                {
                    match &mut element {
                        JavaElement::Method(m) => {
                            m.return_type = self.resolve_type_ref(
                                &m.return_type,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_types,
                            );
                            for param in &mut m.parameters {
                                param.type_ref = self.resolve_type_ref(
                                    &param.type_ref,
                                    parse_result.package_name.as_deref(),
                                    &parse_result.imports,
                                    &known_types,
                                );
                                if let TypeRef::Id(type_fqn) = &param.type_ref {
                                    local_type_map.insert(node.name.to_string(), type_fqn.clone());
                                }
                            }
                        }
                        JavaElement::Field(f) => {
                            f.type_ref = self.resolve_type_ref(
                                &f.type_ref,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_types,
                            );
                            if let TypeRef::Id(type_fqn) = &f.type_ref {
                                local_type_map.insert(node.name.to_string(), type_fqn.clone());
                            }
                        }
                        _ => {}
                    }
                    node.metadata =
                        serde_json::to_value(element).unwrap_or(serde_json::Value::Null);
                }

                unit.add_node(Arc::from(fqn), node.clone());
                if self.is_top_level_node(&node) {
                    unit.add_edge(
                        Arc::from(container_id.as_str()),
                        Arc::from(fqn),
                        GraphEdge::new(EdgeType::Contains),
                    );
                }
            }

            for (source_fqn, target_fqn, edge_type, range) in &parse_result.relations {
                let mut resolved_target = target_fqn.clone();

                if let (Some(tree), Some(source)) = (&parse_result.tree, &parse_result.source) {
                    if let Some(r) = range {
                        let point = tree_sitter::Point::new(r.start_line, r.start_col);
                        if let Some(node) = tree
                            .root_node()
                            .named_descendant_for_point_range(point, point)
                        {
                            let context = ResolutionContext::new_with_unit(
                                node,
                                target_fqn.clone(),
                                &dummy_index,
                                Some(&unit),
                                source,
                                tree,
                                &self.parser,
                            );

                            if let Some(SymbolResolution::Precise(fqn, _)) =
                                self.resolve_symbol_internal(&context)
                            {
                                resolved_target = fqn;
                            } else {
                                if target_fqn.contains('.') {
                                    let parts: Vec<&str> = target_fqn.split('.').collect();
                                    if parts.len() >= 2 {
                                        let obj_name = parts[0];
                                        if let Some(type_fqn) = local_type_map.get(obj_name) {
                                            let mut new_target = type_fqn.clone();
                                            for part in &parts[1..] {
                                                new_target.push('.');
                                                new_target.push_str(part);
                                            }
                                            resolved_target = new_target;
                                        }
                                    }
                                }

                                if !resolved_target.contains('.') {
                                    if let Some(res) = self.parser.resolve_type_name_to_fqn_data(
                                        &resolved_target,
                                        parse_result.package_name.as_deref(),
                                        &parse_result.imports,
                                    ) {
                                        resolved_target = res;
                                    }
                                }
                            }
                        }
                    }
                }

                let edge = GraphEdge::new(edge_type.clone());
                unit.add_edge(
                    Arc::from(source_fqn.as_str()),
                    Arc::from(resolved_target.as_str()),
                    edge,
                );
            }
        }

        Ok(unit)
    }
}
