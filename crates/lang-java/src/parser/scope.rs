use super::JavaParser;
use naviscope_core::model::graph::Range;
use naviscope_core::parser::SymbolIntent;
use naviscope_core::parser::utils::range_from_ts;
use tree_sitter::Node;

impl JavaParser {
    pub fn find_local_declaration(
        &self,
        start_node: Node,
        name: &str,
        source: &str,
    ) -> Option<(Range, Option<String>)> {
        self.find_local_declaration_node(start_node, name, source)
            .map(|(range, type_node)| {
                let type_name = type_node
                    .and_then(|t| t.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()));
                (range, type_name)
            })
    }

    pub fn find_local_declaration_node<'a>(
        &self,
        start_node: Node<'a>,
        name: &str,
        source: &str,
    ) -> Option<(Range, Option<Node<'a>>)> {
        let mut curr = start_node;
        while let Some(parent) = curr.parent() {
            // Check declarations in this scope before or at the start_node (for parameters)
            let mut child_cursor = parent.walk();
            for child in parent.children(&mut child_cursor) {
                if let Some(res) = self.is_decl_of_node(&child, name, source) {
                    // If the declaration is the node itself (like a parameter), or strictly before it
                    if child.start_byte() <= start_node.start_byte() {
                        return Some(res);
                    }
                }
                if child.start_byte() >= start_node.start_byte() {
                    break;
                }
            }
            // Check if parent itself is a declaration (like method parameters)
            if let Some(res) = self.is_decl_of_node(&parent, name, source) {
                return Some(res);
            }
            curr = parent;
        }
        None
    }

    pub fn determine_intent(&self, node: &Node) -> SymbolIntent {
        let parent = match node.parent() {
            Some(p) => p,
            None => return SymbolIntent::Unknown,
        };
        match parent.kind() {
            "method_invocation" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == node.id() {
                        return SymbolIntent::Method;
                    }
                }
                SymbolIntent::Type // Likely the receiver/object
            }
            "method_reference" => SymbolIntent::Type,
            "object_creation_expression" => {
                if let Some(type_node) = parent.child_by_field_name("type") {
                    if type_node.id() == node.id() {
                        return SymbolIntent::Type;
                    }
                }
                SymbolIntent::Unknown
            }
            "type_identifier" | "scoped_identifier" | "scoped_type_identifier" | "generic_type" => {
                SymbolIntent::Type
            }
            "variable_declarator" => SymbolIntent::Variable,
            "field_access" => {
                if let Some(field_node) = parent.child_by_field_name("field") {
                    if field_node.id() == node.id() {
                        return SymbolIntent::Field;
                    }
                }
                SymbolIntent::Type // Likely the receiver/object
            }
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "annotation_type_declaration" => SymbolIntent::Type,
            "method_declaration" | "constructor_declaration" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == node.id() {
                        return SymbolIntent::Method;
                    }
                }
                SymbolIntent::Type
            }
            _ => {
                if node.kind() == "type_identifier" || node.kind() == "scoped_type_identifier" {
                    SymbolIntent::Type
                } else {
                    SymbolIntent::Unknown
                }
            }
        }
    }

    pub fn is_decl_of_node<'a>(
        &self,
        node: &Node<'a>,
        name: &str,
        source: &str,
    ) -> Option<(Range, Option<Node<'a>>)> {
        match node.kind() {
            "variable_declarator" | "formal_parameter" | "catch_formal_parameter" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source.as_bytes()).ok()? == name {
                        let range = range_from_ts(name_node.range());
                        let type_node = if node.kind() == "variable_declarator" {
                            // Type is in the parent local_variable_declaration
                            node.parent().and_then(|p| p.child_by_field_name("type"))
                        } else {
                            // Type is a sibling for parameters
                            node.child_by_field_name("type")
                        };
                        return Some((range, type_node));
                    }
                }
            }
            "local_variable_declaration"
            | "formal_parameters"
            | "inferred_parameters"
            | "enhanced_for_statement"
            | "lambda_expression" => {
                if node.kind() == "lambda_expression" {
                    if let Some(params) = node.child_by_field_name("parameters") {
                        if params.kind() == "identifier" {
                            if params.utf8_text(source.as_bytes()).ok()? == name {
                                return Some((range_from_ts(params.range()), None));
                            }
                        } else {
                            return self.is_decl_of_node(&params, name, source);
                        }
                    }
                    return None;
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(res) = self.is_decl_of_node(&child, name, source) {
                        return Some(res);
                    }
                }
            }
            _ => {}
        }
        None
    }

    pub fn is_decl_of(
        &self,
        node: &Node,
        name: &str,
        source: &str,
    ) -> Option<(Range, Option<String>)> {
        self.is_decl_of_node(node, name, source)
            .map(|(range, type_node)| {
                let type_name = type_node
                    .and_then(|t| t.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()));
                (range, type_name)
            })
    }
}
