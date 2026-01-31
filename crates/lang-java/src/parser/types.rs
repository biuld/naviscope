use super::JavaParser;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

impl JavaParser {
    pub fn parse_type_node(&self, node: Node, source: &str) -> TypeRef {
        match node.kind() {
            "generic_type" => {
                let base_node = node.child_by_field_name("type").or_else(|| node.child(0));

                let base = if let Some(b) = base_node {
                    self.parse_type_node(b, source)
                } else {
                    TypeRef::Unknown
                };

                let mut args = Vec::new();
                // Iterate over children to find type_arguments
                // We manually iterate because child_by_field_name might not catch everything if grammar varies
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if child.kind() == "type_arguments" {
                        let mut args_cursor = child.walk();
                        for arg in child.children(&mut args_cursor) {
                            if !matches!(arg.kind(), "<" | ">" | ",") {
                                args.push(self.parse_type_node(arg, source));
                            }
                        }
                    }
                }

                TypeRef::Generic {
                    base: Box::new(base),
                    args,
                }
            }
            "array_type" => {
                let element_node = node
                    .child_by_field_name("element")
                    .or_else(|| node.child(0));

                let element = if let Some(e) = element_node {
                    self.parse_type_node(e, source)
                } else {
                    TypeRef::Unknown
                };

                let dim_node = node.child_by_field_name("dimensions");
                let count = if let Some(d) = dim_node {
                    d.utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .matches('[')
                        .count()
                } else {
                    1
                };

                TypeRef::Array {
                    element: Box::new(element),
                    dimensions: count,
                }
            }
            "wildcard" => {
                // Check for bounds
                let mut bound = None;
                let mut is_upper = true;

                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    match child.kind() {
                        "super" => is_upper = false,
                        "extends" => is_upper = true,
                        k if k != "?" => {
                            // Assume this is the type bound
                            bound = Some(Box::new(self.parse_type_node(child, source)));
                        }
                        _ => {}
                    }
                }

                TypeRef::Wildcard {
                    bound,
                    is_upper_bound: is_upper,
                }
            }
            _ => {
                let text = node
                    .utf8_text(source.as_bytes())
                    .unwrap_or_default()
                    .to_string();
                if text.is_empty() {
                    TypeRef::Unknown
                } else {
                    TypeRef::Raw(text)
                }
            }
        }
    }
}
