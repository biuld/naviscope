use crate::resolver::{LangResolver, ProjectContext};
use crate::error::Result;
use crate::model::graph::{EdgeType, GraphEdge, GraphNode, ResolvedUnit};
use crate::resolver::SemanticResolver;
use crate::index::NaviscopeIndex;
use crate::project::scanner::{ParsedContent, ParsedFile};
use crate::parser::{SymbolResolution, matches_intent};
use crate::parser::SymbolIntent;
use crate::parser::java::JavaParser;
use petgraph::stable_graph::NodeIndex;
use tree_sitter::Tree;

use crate::model::lang::java::JavaElement;

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

    /// Check if a node is top-level (class, interface, enum, annotation)
    fn is_top_level_node(&self, node: &GraphNode) -> bool {
        let kind = node.kind();
        kind == "class" || kind == "interface" || kind == "enum" || kind == "annotation"
    }

    /// Finds nodes by name and intent within the index
    fn find_by_name_and_intent(&self, index: &NaviscopeIndex, name: &str, intent: SymbolIntent) -> Vec<NodeIndex> {
        let mut results = Vec::new();
        if let Some(indices) = index.name_map.get(name) {
            for &idx in indices {
                if let Some(node) = index.graph.node_weight(idx) {
                    if matches_intent(node.kind(), intent) {
                        results.push(idx);
                    }
                }
            }
        }
        results
    }
}

impl SemanticResolver for JavaResolver {
    fn resolve_at(&self, tree: &Tree, source: &str, line: usize, byte_col: usize) -> Option<SymbolResolution> {
        let point = tree_sitter::Point::new(line, byte_col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)?;

        let kind = node.kind();
        if kind != "identifier" && kind != "type_identifier" && kind != "scoped_identifier" {
            return None;
        }

        let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
        let intent = self.parser.determine_intent(&node);

        // 1. Try to find local declaration by climbing up
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            if let Some(decl_range) = self.parser.is_decl_of(&curr, &name, source) {
                return Some(SymbolResolution::Local(decl_range));
            }

            let mut child_cursor = parent.walk();
            for child in parent.children(&mut child_cursor) {
                if child.start_byte() >= node.start_byte() {
                    break;
                }
                if let Some(decl_range) = self.parser.is_decl_of(&child, &name, source) {
                    return Some(SymbolResolution::Local(decl_range));
                }
            }
            curr = parent;
        }

        // 2. Precise resolution for Methods and Fields: Try to resolve receiver type
        if intent == SymbolIntent::Method || intent == SymbolIntent::Field {
            if let Some(parent) = node.parent() {
                let receiver_node = match parent.kind() {
                    "method_invocation" | "field_access" => parent.child_by_field_name("object"),
                    _ => None,
                };

                if let Some(receiver) = receiver_node {
                    if let Some(receiver_type_fqn) = self.parser.resolve_receiver_type(&receiver, tree, source) {
                        return Some(SymbolResolution::Precise(format!("{}.{}", receiver_type_fqn, name), intent));
                    }
                } else {
                    // Implicit this
                    let (pkg, _) = self.parser.extract_package_and_imports(tree, source);
                    if let Some(class_fqn) = self.parser.find_enclosing_class_fqn(&node, source, pkg.as_deref()) {
                        return Some(SymbolResolution::Precise(format!("{}.{}", class_fqn, name), intent));
                    }
                }
            }
        }

        // 3. Resolve via imports & package
        let (pkg, imports) = self.parser.extract_package_and_imports(tree, source);

        if intent == SymbolIntent::Type {
            if let Some(enclosing_fqn) = self.parser.find_enclosing_class_fqn(&node, source, pkg.as_deref()) {
                return Some(SymbolResolution::Precise(format!("{}.{}", enclosing_fqn, name), intent));
            }
        }

        for imp in &imports {
            if imp.ends_with(&format!(".{}", name)) {
                return Some(SymbolResolution::Precise(imp.clone(), intent));
            }
        }

        if let Some(p) = pkg {
            return Some(SymbolResolution::Precise(format!("{}.{}", p, name), intent));
        }

        // 4. Fallback to heuristic
        Some(SymbolResolution::Heuristic(name, intent))
    }

    fn find_matches(&self, index: &NaviscopeIndex, resolution: &SymbolResolution) -> Vec<NodeIndex> {
        match resolution {
            SymbolResolution::Local(_) => vec![], // Handled by Document locally
            SymbolResolution::Precise(fqn, intent) => {
                // 1. Try precise FQN match
                if let Some(&idx) = index.fqn_map.get(fqn) {
                    if let Some(node) = index.graph.node_weight(idx) {
                        if matches_intent(node.kind(), *intent) {
                            return vec![idx];
                        }
                    }
                }

                // 2. Fallback to name-based search if FQN match fails
                let name = fqn.split('.').last().unwrap_or(fqn);
                self.find_by_name_and_intent(index, name, *intent)
            }
            SymbolResolution::Heuristic(name, intent) => {
                self.find_by_name_and_intent(index, name, *intent)
            }
        }
    }

    fn resolve_type_of(&self, index: &NaviscopeIndex, resolution: &SymbolResolution) -> Vec<SymbolResolution> {
        let mut type_resolutions = Vec::new();

        match resolution {
            SymbolResolution::Local(_) => {
                // Currently, we don't have enough context to find the local variable's type here
                // without the original word or document. 
                // For now, return empty or we might need to change the API.
            }
            SymbolResolution::Precise(fqn, intent) => {
                if let Some(&idx) = index.fqn_map.get(fqn) {
                    let node = &index.graph[idx];
                    if let crate::model::graph::GraphNode::Code(
                        crate::model::graph::CodeElement::Java { element, .. },
                    ) = node
                    {
                        match element {
                            JavaElement::Field(f) => {
                                type_resolutions.push(SymbolResolution::Precise(f.type_name.clone(), SymbolIntent::Type))
                            }
                            JavaElement::Method(m) => {
                                type_resolutions.push(SymbolResolution::Precise(m.return_type.clone(), SymbolIntent::Type))
                            }
                            _ => {
                                // If it's already a type, it's its own type
                                if matches_intent(node.kind(), SymbolIntent::Type) {
                                    type_resolutions.push(resolution.clone());
                                }
                            }
                        }
                    }
                } else {
                    // Fallback: if it's precise but not in index, and it looks like a type intent
                    if *intent == SymbolIntent::Type {
                        type_resolutions.push(resolution.clone());
                    }
                }
            }
            SymbolResolution::Heuristic(name, intent) => {
                if let Some(nodes) = index.name_map.get(name) {
                    for &idx in nodes {
                        let node = &index.graph[idx];
                        if let crate::model::graph::GraphNode::Code(
                            crate::model::graph::CodeElement::Java { element, .. },
                        ) = node
                        {
                            match element {
                                JavaElement::Field(f) => {
                                    type_resolutions.push(SymbolResolution::Precise(f.type_name.clone(), SymbolIntent::Type))
                                }
                                JavaElement::Method(m) => {
                                    type_resolutions.push(SymbolResolution::Precise(m.return_type.clone(), SymbolIntent::Type))
                                }
                                _ => {
                                    if matches_intent(node.kind(), SymbolIntent::Type) {
                                        type_resolutions.push(SymbolResolution::Heuristic(name.clone(), *intent));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        type_resolutions
    }

    fn find_implementations(&self, index: &NaviscopeIndex, resolution: &SymbolResolution) -> Vec<NodeIndex> {
        let target_nodes = self.find_matches(index, resolution);
        let mut results = Vec::new();

        for &node_idx in &target_nodes {
            let mut incoming = index
                .graph
                .neighbors_directed(node_idx, petgraph::Direction::Incoming)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&index.graph) {
                let edge = &index.graph[edge_idx];
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

            // Step 3: Add pre-built nodes and link to container
            for node in &parse_result.nodes {
                let fqn = node.fqn();
                unit.add_node(fqn.to_string(), node.clone());

                if self.is_top_level_node(node) {
                    unit.add_edge(container_id.clone(), fqn.to_string(), GraphEdge::new(EdgeType::Contains));
                }
            }

            // Step 4: Add relations with FQN resolution
            for (source_fqn, target_fqn, edge_type, range) in &parse_result.relations {
                let mut resolved_target = target_fqn.clone();

                // Unified FQN resolution
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
