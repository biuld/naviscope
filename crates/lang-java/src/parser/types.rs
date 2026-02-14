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
            "integral_type"
            | "floating_point_type"
            | "boolean_type"
            | "void_type"
            | "_unannotated_type" => {
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
            "type_identifier" | "scoped_type_identifier" => {
                let text = node
                    .utf8_text(source.as_bytes())
                    .unwrap_or_default()
                    .to_string();
                if text.is_empty() {
                    TypeRef::Unknown
                } else {
                    // At parser stage these identifiers are syntactic type names and may not
                    // be fully resolved; keep them as Raw and normalize later in semantic phase.
                    TypeRef::Raw(text)
                }
            }
            _ => {
                let text = node
                    .utf8_text(source.as_bytes())
                    .unwrap_or_default()
                    .to_string();
                // Default fallback
                if text.is_empty() {
                    TypeRef::Unknown
                } else {
                    // Start uppercase -> Id, lowercase -> Raw? Or just Raw (safer for primitives missed)
                    // If it ends with 'type', it's likely primitive node we missed?
                    // Safe logic: If primitive keyword -> Raw. Else Id.
                    // But we can just use Raw for now as fallback.
                    TypeRef::Raw(text)
                }
            }
        }
    }

    /// Extract parameter types from a method/constructor declaration node.
    ///
    /// This reads the `formal_parameters` child directly from the AST so that
    /// we can build signature-based FQNs at ID-generation time, **before** the
    /// enrichment stage populates `JavaIndexMetadata.parameters`.
    ///
    /// For varargs (`spread_parameter`), the type is wrapped in `TypeRef::Array`
    /// to match Java's bytecode representation.
    pub fn extract_param_types_from_declaration(
        &self,
        declaration_node: Node,
        source: &str,
    ) -> Vec<TypeRef> {
        self.extract_method_parameters(declaration_node, source)
            .into_iter()
            .map(|p| p.type_ref)
            .collect()
    }

    /// Extracts full parameter metadata for parsing.
    pub fn extract_method_parameters(
        &self,
        declaration_node: tree_sitter::Node,
        source: &str,
    ) -> Vec<crate::model::JavaParameter> {
        let Some(params_node) = declaration_node.child_by_field_name("parameters") else {
            return vec![];
        };

        let mut result = Vec::new();
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "formal_parameter" => {
                    if let Some(type_node) = child.child_by_field_name("type") {
                        let type_ref = self.parse_type_node(type_node, source);
                        let name_node = child.child_by_field_name("name");
                        let name = name_node
                            .and_then(|n| n.utf8_text(source.as_bytes()).ok())
                            .unwrap_or("arg")
                            .to_string();

                        result.push(crate::model::JavaParameter {
                            name,
                            type_ref,
                            is_varargs: false,
                        });
                    }
                }
                "spread_parameter" => {
                    let mut type_ref = naviscope_api::models::TypeRef::Unknown;
                    let mut name = "arg".to_string();

                    // Find type (first named node that is not variable_declarator)
                    // Find name (in variable_declarator)
                    let mut inner_cursor = child.walk();
                    for gc in child.children(&mut inner_cursor) {
                        if gc.kind() == "variable_declarator" {
                            if let Some(n) = gc.child_by_field_name("name") {
                                if let Ok(text) = n.utf8_text(source.as_bytes()) {
                                    name = text.to_string();
                                }
                            }
                        } else if gc.kind() != "..." && gc.is_named() {
                            let base = self.parse_type_node(gc, source);
                            type_ref = crate::naming::varargs_to_array_type(&base);
                        }
                    }

                    result.push(crate::model::JavaParameter {
                        name,
                        type_ref,
                        is_varargs: true,
                    });
                }
                _ => {}
            }
        }
        result
    }
}
