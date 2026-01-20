use crate::parser::{LspParser, SymbolResolution};
use crate::parser::utils::{RawSymbol, build_symbol_hierarchy};
use tree_sitter::Tree;
use super::JavaParser;

impl LspParser for JavaParser {
    fn parse(&self, source: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language).ok()?;
        parser.parse(source, old_tree)
    }

    fn resolve_symbol(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        byte_col: usize,
    ) -> Option<SymbolResolution> {
        let point = tree_sitter::Point::new(line, byte_col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)?;

        let kind = node.kind();
        if kind != "identifier" && kind != "type_identifier" && kind != "scoped_identifier" {
            return None;
        }

        let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
        let intent = self.determine_intent(&node);

        // 1. Try to find local declaration by climbing up
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            // Check if current node is the declaration itself
            if let Some(decl_range) = self.is_decl_of(&curr, &name, source) {
                return Some(SymbolResolution::Local(decl_range));
            }

            let mut child_cursor = parent.walk();
            for child in parent.children(&mut child_cursor) {
                if child.start_byte() >= node.start_byte() {
                    break;
                }
                if let Some(decl_range) = self.is_decl_of(&child, &name, source) {
                    return Some(SymbolResolution::Local(decl_range));
                }
            }
            curr = parent;
        }

        // 2. Precise resolution for Methods and Fields: Try to resolve receiver type
        if intent == crate::parser::SymbolIntent::Method || intent == crate::parser::SymbolIntent::Field {
            if let Some(parent) = node.parent() {
                let receiver_node = match parent.kind() {
                    "method_invocation" | "field_access" => parent.child_by_field_name("object"),
                    _ => None,
                };

                if let Some(receiver) = receiver_node {
                    if let Some(receiver_type_fqn) = self.resolve_receiver_type(&receiver, tree, source) {
                        return Some(SymbolResolution::Precise(format!("{}.{}", receiver_type_fqn, name), intent));
                    }
                } else {
                    // Implicit this
                    let (pkg, _) = self.extract_package_and_imports(tree, source);
                    if let Some(class_fqn) = self.find_enclosing_class_fqn(&node, source, pkg.as_deref()) {
                        return Some(SymbolResolution::Precise(format!("{}.{}", class_fqn, name), intent));
                    }
                }
            }
        }

        // 3. Resolve via imports & package
        let (pkg, imports) = self.extract_package_and_imports(tree, source);

        // Special handling for Types: Check if it's an inner class of the current class
        if intent == crate::parser::SymbolIntent::Type {
            if let Some(enclosing_fqn) = self.find_enclosing_class_fqn(&node, source, pkg.as_deref()) {
                // If it's an inner class, the FQN should be OuterClass.InnerClass
                // We return this as a precise match. If it's not actually an inner class,
                // the index fallback will still find it by name.
                return Some(SymbolResolution::Precise(
                    format!("{}.{}", enclosing_fqn, name),
                    intent,
                ));
            }
        }

        for imp in &imports {
            let imp_str: &str = imp;
            if imp_str.ends_with(&format!(".{}", name)) {
                return Some(SymbolResolution::Precise(imp.clone(), intent));
            }
        }

        if let Some(p) = pkg {
            return Some(SymbolResolution::Precise(format!("{}.{}", p, name), intent));
        }

        // 4. Fallback to heuristic
        Some(SymbolResolution::Heuristic(name, intent))
    }

    fn extract_symbols(&self, tree: &Tree, source: &str) -> Vec<crate::parser::DocumentSymbol> {
        // Use the native AST analyzer
        let model = self.analyze(tree, source);
        
        // Convert JavaEntity to RawSymbol for the tree builder
        let raw_symbols = model.entities
            .into_iter()
            .map(|e| {
                let kind = match e.element {
                    crate::model::lang::java::JavaElement::Class(_) => "class",
                    crate::model::lang::java::JavaElement::Interface(_) => "interface",
                    crate::model::lang::java::JavaElement::Enum(_) => "enum",
                    crate::model::lang::java::JavaElement::Annotation(_) => "annotation",
                    crate::model::lang::java::JavaElement::Method(ref m) => if m.is_constructor { "constructor" } else { "method" },
                    crate::model::lang::java::JavaElement::Field(_) => "field",
                };
                
                RawSymbol {
                    name: e.element.name().to_string(),
                    kind: kind.to_string(),
                    range: e.element.range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
                    selection_range: e.element.name_range().cloned().unwrap_or(crate::model::graph::Range { start_line: 0, start_col: 0, end_line: 0, end_col: 0 }),
                    node: e.node,
                }
            })
            .collect();

        build_symbol_hierarchy(raw_symbols)
    }
}
