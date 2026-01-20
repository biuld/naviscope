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

    fn resolve_expression_type(&self, node: &tree_sitter::Node, tree: &Tree, source: &str, index: &NaviscopeIndex) -> Option<String> {
        let kind = node.kind();
        match kind {
            "identifier" => {
                let name = node.utf8_text(source.as_bytes()).ok()?;
                
                // 1. Local Scope
                if let Some((_, Some(type_name))) = self.parser.find_local_declaration(*node, name, source) {
                    return self.parser.resolve_type_name_to_fqn(type_name.as_str(), tree, source);
                }

                // 2. Lexical Scope (Implicit this fields/members or Nested Types)
                let (pkg, _) = self.parser.extract_package_and_imports(tree, source);
                for container_fqn in self.parser.get_enclosing_class_fqns(node, source, pkg.as_deref()) {
                    let candidate = format!("{}.{}", container_fqn, name);
                    if index.fqn_map.contains_key(&candidate) {
                        return Some(candidate); // It's a member of an outer class
                    }
                }

                // 3. Global Scope (Static access to a class)
                self.parser.resolve_type_name_to_fqn(name, tree, source)
            }
            "field_access" => {
                let receiver = node.child_by_field_name("object")?;
                let field_name = node.child_by_field_name("field")?.utf8_text(source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, tree, source, index)?;
                let field_fqn = format!("{}.{}", receiver_type, field_name);
                if let Some(&idx) = index.fqn_map.get(&field_fqn) {
                    let node = &index.graph[idx];
                    if let crate::model::graph::GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Field(f), .. }) = node {
                        return self.parser.resolve_type_name_to_fqn(&f.type_name, tree, source);
                    }
                }
                None
            }
            "method_invocation" => {
                let receiver = node.child_by_field_name("object")?;
                let method_name = node.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, tree, source, index)?;
                let method_fqn = format!("{}.{}", receiver_type, method_name);
                if let Some(&idx) = index.fqn_map.get(&method_fqn) {
                    let node = &index.graph[idx];
                    if let crate::model::graph::GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Method(m), .. }) = node {
                        return self.parser.resolve_type_name_to_fqn(&m.return_type, tree, source);
                    }
                }
                None
            }
            "this" => {
                let (pkg, _) = self.parser.extract_package_and_imports(tree, source);
                self.parser.get_enclosing_class_fqns(node, source, pkg.as_deref()).first().cloned()
            }
            "scoped_type_identifier" | "scoped_identifier" => {
                let receiver = node.child_by_field_name("scope")?;
                let name = node.child_by_field_name("name")?.utf8_text(source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, tree, source, index)?;
                Some(format!("{}.{}", receiver_type, name))
            }
            _ => None
        }
    }
}

impl SemanticResolver for JavaResolver {
    fn resolve_at(&self, tree: &Tree, source: &str, line: usize, byte_col: usize, index: &NaviscopeIndex) -> Option<SymbolResolution> {
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

        // Check for explicit receiver
        let receiver_node = if let Some(parent) = node.parent() {
            match parent.kind() {
                "field_access" | "method_invocation" => {
                    parent.child_by_field_name("object")
                        .filter(|obj| obj.id() != node.id())
                }
                "scoped_type_identifier" | "scoped_identifier" => {
                    parent.child_by_field_name("scope")
                        .filter(|obj| obj.id() != node.id())
                }
                _ => None,
            }
        } else {
            None
        };

        // 1. Local Scope (Only if no explicit receiver)
        if receiver_node.is_none() {
            if let Some((decl_range, type_name)) = self.parser.find_local_declaration(node, &name, source) {
                return Some(SymbolResolution::Local(decl_range, type_name));
            }
        }

        // 2. Member Access (Explicit Receiver)
        if let Some(receiver) = receiver_node {
            if let Some(receiver_type_fqn) = self.resolve_expression_type(&receiver, tree, source, index) {
                return Some(SymbolResolution::Precise(format!("{}.{}", receiver_type_fqn, name), intent));
            }
        }

        // 3. Lexical Scope (Implicit this or Nested Types)
        let (pkg, imports) = self.parser.extract_package_and_imports(tree, source);
        let enclosing_fqns = self.parser.get_enclosing_class_fqns(&node, source, pkg.as_deref());
        
        // Check if name is a member or nested type of any enclosing class
        for container_fqn in &enclosing_fqns {
            let candidate = format!("{}.{}", container_fqn, name);
            if index.fqn_map.contains_key(&candidate) {
                return Some(SymbolResolution::Precise(candidate, intent));
            }
        }

        // 4. Structural check (Is this node itself a definition name?)
        if let Some(parent) = node.parent() {
            if let Some(n_node) = parent.child_by_field_name("name") {
                if n_node.id() == node.id() {
                    let fqn = self.parser.get_fqn_for_definition(&node, source, pkg.as_deref());
                    if index.fqn_map.contains_key(&fqn) {
                        return Some(SymbolResolution::Precise(fqn, intent));
                    }
                }
            }
        }

        // 5. Global Scope (Imports, java.lang, current package)
        if intent == SymbolIntent::Type {
            if let Some(fqn) = self.parser.resolve_type_name_to_fqn_data(&name, pkg.as_deref(), &imports) {
                return Some(SymbolResolution::Precise(fqn, intent));
            }
        } else {
            // Precise imports for methods/fields (static imports not yet supported, but common for types)
            for imp in &imports {
                if imp.ends_with(&format!(".{}", name)) {
                    return Some(SymbolResolution::Precise(imp.clone(), intent));
                }
            }
            if let Some(p) = &pkg {
                let candidate = format!("{}.{}", p, name);
                if index.fqn_map.contains_key(&candidate) {
                    return Some(SymbolResolution::Precise(candidate, intent));
                }
            }
        }

        None
    }

    fn find_matches(&self, index: &NaviscopeIndex, resolution: &SymbolResolution) -> Vec<NodeIndex> {
        match resolution {
            SymbolResolution::Local(_, _) => vec![], // Handled by Document locally
            SymbolResolution::Precise(fqn, intent) => {
                // 1. Try precise FQN match
                if let Some(&idx) = index.fqn_map.get(fqn) {
                    if let Some(node) = index.graph.node_weight(idx) {
                        // Trust FQN matches even if intent is Unknown
                        if *intent == SymbolIntent::Unknown || matches_intent(node.kind(), *intent) {
                            return vec![idx];
                        }
                    }
                }
                vec![]
            }
        }
    }

    fn resolve_type_of(&self, index: &NaviscopeIndex, resolution: &SymbolResolution) -> Vec<SymbolResolution> {
        let mut type_resolutions = Vec::new();

        match resolution {
            SymbolResolution::Local(_, type_name) => {
                if let Some(tn) = type_name {
                    // Resolve the type name to an FQN
                    if let Some(fqn) = self.parser.resolve_type_name_to_fqn_data(tn, None, &[]) {
                         type_resolutions.push(SymbolResolution::Precise(fqn, SymbolIntent::Type));
                    }
                }
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
