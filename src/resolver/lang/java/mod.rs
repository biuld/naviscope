use crate::resolver::{LangResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode, ResolvedUnit};
use crate::resolver::SemanticResolver;
use crate::index::CodeGraph;
use crate::project::scanner::{ParsedContent, ParsedFile};
use crate::parser::{SymbolResolution, matches_intent};
use crate::parser::SymbolIntent;
use crate::parser::java::JavaParser;
use petgraph::stable_graph::NodeIndex;
use tree_sitter::Tree;
use std::ops::ControlFlow;

pub mod context;
pub mod scope;

use context::ResolutionContext;
use scope::{Scope, LocalScope, MemberScope, ImportScope, BuiltinScope};

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
        kind == "class" || kind == "interface" || kind == "enum" || kind == "annotation"
    }

    fn get_active_scopes<'a>(&'a self, ctx: &'a ResolutionContext) -> Vec<Box<dyn Scope + 'a>> {
        let mut scopes: Vec<Box<dyn Scope + 'a>> = Vec::new();

        if ctx.receiver_node.is_none() {
            scopes.push(Box::new(LocalScope { parser: &self.parser }));
        }

        scopes.push(Box::new(MemberScope { parser: &self.parser }));
        scopes.push(Box::new(ImportScope { parser: &self.parser }));

        if ctx.intent == SymbolIntent::Type {
            scopes.push(Box::new(BuiltinScope { parser: &self.parser }));
        }

        scopes
    }
}

impl SemanticResolver for JavaResolver {
    fn resolve_at(&self, tree: &Tree, source: &str, line: usize, byte_col: usize, index: &CodeGraph) -> Option<SymbolResolution> {
        let point = tree_sitter::Point::new(line, byte_col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
            .filter(|n| matches!(n.kind(), "identifier" | "type_identifier" | "scoped_identifier" | "this"))?;

        let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
        let context = ResolutionContext::new(node, name, index, source, tree, &self.parser);

        match self.get_active_scopes(&context)
            .into_iter()
            .try_fold(None, |_: Option<SymbolResolution>, scope| {
                match scope.resolve(&context.name, &context) {
                    Some(Ok(res)) => ControlFlow::Break(Some(res)),
                    Some(Err(())) => ControlFlow::Break(None),
                    None => ControlFlow::Continue(None),
                }
            }) {
            ControlFlow::Break(res) => res,
            ControlFlow::Continue(_) => None,
        }
    }

    fn find_matches(&self, index: &CodeGraph, resolution: &SymbolResolution) -> Vec<NodeIndex> {
        match resolution {
            SymbolResolution::Local(_, _) => vec![],
            SymbolResolution::Precise(fqn, intent) => {
                if let Some(&idx) = index.fqn_map.get(fqn) {
                    if let Some(node) = index.topology.node_weight(idx) {
                        if *intent == SymbolIntent::Unknown || matches_intent(node.kind(), *intent) {
                            return vec![idx];
                        }
                    }
                }
                vec![]
            }
        }
    }

    fn resolve_type_of(&self, index: &CodeGraph, resolution: &SymbolResolution) -> Vec<SymbolResolution> {
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
                if let Some(&idx) = index.fqn_map.get(fqn) {
                    let node = &index.topology[idx];
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java { element, .. }) = node {
                        match element {
                            crate::model::lang::java::JavaElement::Field(f) => {
                                type_resolutions.push(SymbolResolution::Precise(f.type_name.clone(), SymbolIntent::Type))
                            }
                            crate::model::lang::java::JavaElement::Method(m) => {
                                type_resolutions.push(SymbolResolution::Precise(m.return_type.clone(), SymbolIntent::Type))
                            }
                            _ => {
                                if matches_intent(node.kind(), SymbolIntent::Type) {
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

    fn find_implementations(&self, index: &CodeGraph, resolution: &SymbolResolution) -> Vec<NodeIndex> {
        let target_nodes = self.find_matches(index, resolution);
        let mut results = Vec::new();

        for &node_idx in &target_nodes {
            let mut incoming = index
                .topology
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.topology) {
                let edge = &index.topology[edge_idx];
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

        if let ParsedContent::Java(parse_result) = &file.content {
            let module_id = context
                .find_module_for_path(&file.file.path)
                .unwrap_or_else(|| "module::root".to_string());

            let container_id = if let Some(pkg_name) = &parse_result.package_name {
                let package_id = format!("{}::{}", module_id, pkg_name);
                unit.add_node(
                    package_id.clone(),
                    GraphNode::gradle(
                        crate::model::lang::gradle::GradleElement::Package(
                            crate::model::lang::gradle::GradlePackage {
                                name: pkg_name.clone(),
                                id: package_id.clone(),
                            },
                        ),
                        None,
                    ),
                );
                unit.add_edge(module_id, package_id.clone(), GraphEdge::new(EdgeType::Contains));
                package_id
            } else {
                module_id
            };

            for node in &parse_result.nodes {
                let fqn = node.fqn();
                unit.add_node(fqn.to_string(), node.clone());
                if self.is_top_level_node(node) {
                    unit.add_edge(container_id.clone(), fqn.to_string(), GraphEdge::new(EdgeType::Contains));
                }
            }

            for (source_fqn, target_fqn, edge_type, range) in &parse_result.relations {
                let mut resolved_target = target_fqn.clone();
                if !target_fqn.contains('.') {
                    if let Some(res) = self.parser.resolve_type_name_to_fqn_data(
                        target_fqn,
                        parse_result.package_name.as_deref(),
                        &parse_result.imports,
                    ) {
                        resolved_target = res;
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
