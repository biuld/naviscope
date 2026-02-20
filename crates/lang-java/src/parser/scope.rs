use super::JavaParser;
use naviscope_api::models::SymbolIntent;
use tree_sitter::Node;

impl JavaParser {
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
                SymbolIntent::Unknown // Could be variable, field, or type - resolver will determine
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
                SymbolIntent::Unknown // Could be variable, field, or type - resolver will determine
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
}
