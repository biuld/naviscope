use crate::engine::CodeGraph;
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode, GraphOp, NodeKind, ResolvedUnit};
use crate::model::lang::java::{JavaElement, JavaPackage};
use crate::model::signature::TypeRef;
use crate::parser::SymbolIntent;
use crate::parser::java::JavaParser;
use crate::parser::{SymbolResolution, matches_intent};
use crate::project::scanner::{ParsedContent, ParsedFile};
use crate::query::CodeGraphLike;
use crate::resolver::SemanticResolver;
use crate::resolver::{LangResolver, ProjectContext};
use petgraph::stable_graph::NodeIndex;
use std::ops::ControlFlow;
use tree_sitter::Tree;

pub mod context;
pub mod scope;

use context::ResolutionContext;
use scope::{BuiltinScope, ImportScope, LocalScope, MemberScope, Scope};

#[derive(Clone)]
pub struct JavaResolver {
    parser: JavaParser,
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
                if let Some(&idx) = index.fqn_map().get(fqn) {
                    if let Some(node) = index.topology().node_weight(idx) {
                        if *intent == SymbolIntent::Unknown || matches_intent(&node.kind(), *intent)
                        {
                            return vec![idx];
                        }
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
                if let Some(&idx) = index.fqn_map().get(fqn) {
                    let node = &index.topology()[idx];
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java {
                        element, ..
                    }) = node
                    {
                        match element {
                            crate::model::lang::java::JavaElement::Field(f) => match &f.type_ref {
                                crate::model::signature::TypeRef::Raw(s) => type_resolutions
                                    .push(SymbolResolution::Precise(s.clone(), SymbolIntent::Type)),
                                crate::model::signature::TypeRef::Id(id) => type_resolutions.push(
                                    SymbolResolution::Precise(id.clone(), SymbolIntent::Type),
                                ),
                                _ => {}
                            },
                            crate::model::lang::java::JavaElement::Method(m) => {
                                match &m.return_type {
                                    crate::model::signature::TypeRef::Raw(s) => type_resolutions
                                        .push(SymbolResolution::Precise(
                                            s.clone(),
                                            SymbolIntent::Type,
                                        )),
                                    crate::model::signature::TypeRef::Id(id) => type_resolutions
                                        .push(SymbolResolution::Precise(
                                            id.clone(),
                                            SymbolIntent::Type,
                                        )),
                                    _ => {}
                                }
                            }
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
            if let GraphNode::Code(crate::model::graph::CodeElement::Java { element, .. }) = node {
                if let crate::model::lang::java::JavaElement::Method(m) = element {
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
                                        if let GraphNode::Code(
                                            crate::model::graph::CodeElement::Java {
                                                element:
                                                    crate::model::lang::java::JavaElement::Method(
                                                        child_m,
                                                    ),
                                                ..
                                            },
                                        ) = &index.topology()[child_idx]
                                        {
                                            if child_m.name == m.name {
                                                results.push(child_idx);
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

        if let ParsedContent::Java(parse_result) = &file.content {
            unit.identifiers = parse_result.identifiers.clone();
            unit.ops.push(GraphOp::UpdateIdentifiers {
                path: file.file.path.clone(),
                identifiers: parse_result.identifiers.clone(),
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

                // Create package node
                unit.add_node(
                    package_id.clone(),
                    GraphNode::java(
                        JavaElement::Package(JavaPackage {
                            name: pkg_name.clone(),
                            id: package_id.clone(),
                        }),
                        None,
                    ),
                );

                // Link package to module
                unit.add_edge(
                    module_id.clone(),
                    package_id.clone(),
                    GraphEdge::new(EdgeType::Contains),
                );

                package_id
            } else {
                module_id
            };

            let mut known_fqns = std::collections::HashSet::new();
            let mut local_type_map = std::collections::HashMap::new();

            for node in &parse_result.nodes {
                known_fqns.insert(node.fqn().to_string());
            }

            for node in &parse_result.nodes {
                let fqn = node.fqn();
                let mut node = node.clone();

                // Enhance node with resolved types
                if let GraphNode::Code(crate::model::graph::CodeElement::Java { element, .. }) =
                    &mut node
                {
                    match element {
                        crate::model::lang::java::JavaElement::Method(m) => {
                            m.return_type = self.resolve_type_ref(
                                &m.return_type,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_fqns,
                            );
                            for param in &mut m.parameters {
                                param.type_ref = self.resolve_type_ref(
                                    &param.type_ref,
                                    parse_result.package_name.as_deref(),
                                    &parse_result.imports,
                                    &known_fqns,
                                );
                                if let TypeRef::Id(type_fqn) = &param.type_ref {
                                    local_type_map.insert(param.name.clone(), type_fqn.clone());
                                }
                            }
                        }
                        crate::model::lang::java::JavaElement::Field(f) => {
                            f.type_ref = self.resolve_type_ref(
                                &f.type_ref,
                                parse_result.package_name.as_deref(),
                                &parse_result.imports,
                                &known_fqns,
                            );
                            if let TypeRef::Id(type_fqn) = &f.type_ref {
                                local_type_map.insert(f.name.clone(), type_fqn.clone());
                            }
                        }
                        _ => {}
                    }
                }

                unit.add_node(fqn.to_string(), node.clone());
                if self.is_top_level_node(&node) {
                    unit.add_edge(
                        container_id.clone(),
                        fqn.to_string(),
                        GraphEdge::new(EdgeType::Contains),
                    );
                }
            }

            for (source_fqn, target_fqn, edge_type, range) in &parse_result.relations {
                let mut resolved_target = target_fqn.clone();

                // If we have a tree and source, we can use the Scope system!
                if let (Some(tree), Some(source)) = (&parse_result.tree, &parse_result.source) {
                    if let Some(r) = range {
                        let point = tree_sitter::Point::new(r.start_line, r.start_col);
                        if let Some(node) = tree
                            .root_node()
                            .named_descendant_for_point_range(point, point)
                        {
                            // Now we have a Node! We can build a ResolutionContext and run Scopes.
                            // We provide the current unit so that MemberScope can see nodes we just added.
                            let context = ResolutionContext::new_with_unit(
                                node,
                                target_fqn.clone(),
                                &dummy_index,
                                Some(&unit),
                                source,
                                tree,
                                &self.parser,
                            );

                            // Run the same scope chain as resolve_at
                            if let Some(SymbolResolution::Precise(fqn, _)) =
                                self.resolve_symbol_internal(&context)
                            {
                                resolved_target = fqn;
                            } else {
                                // Fallback A: Try resolving via local_type_map (handles obj.method)
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

                                // Fallback B: Basic type-to-fqn resolution
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
                unit.add_edge(source_fqn.clone(), resolved_target, edge);
            }
        }

        Ok(unit)
    }
}
