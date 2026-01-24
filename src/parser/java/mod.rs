use crate::error::Result;
use crate::model::graph::Range;
use crate::model::signature::TypeRef;
use crate::parser::utils::range_from_ts;
use crate::parser::SymbolIntent;
use tree_sitter::{Node, Query, Tree, StreamingIterator};
use std::sync::Arc;

mod constants;
mod lsp;
mod index;
mod ast;

unsafe extern "C" {
    fn tree_sitter_java() -> tree_sitter::Language;
}

use crate::parser::queries::java_definitions::JavaIndices;

pub struct JavaParser {
    pub language: tree_sitter::Language,
    pub(crate) definition_query: Arc<Query>,
    pub(crate) indices: JavaIndices,
}

impl Clone for JavaParser {
    fn clone(&self) -> Self {
        Self {
            language: self.language.clone(),
            definition_query: Arc::clone(&self.definition_query),
            indices: self.indices.clone(),
        }
    }
}

impl JavaParser {
    pub fn new() -> Result<Self> {
        let language = unsafe { tree_sitter_java() };
        let definition_query = crate::parser::utils::load_query(
            &language,
            include_str!("../queries/java_definitions.scm"),
        )?;
        let indices = JavaIndices::new(&definition_query)?;

        Ok(Self {
            language,
            definition_query: Arc::new(definition_query),
            indices,
        })
    }

    pub fn parse_type_node(&self, node: Node, source: &str) -> TypeRef {
        match node.kind() {
            "generic_type" => {
                let base_node = node.child_by_field_name("type")
                    .or_else(|| node.child(0));
                
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
            },
            "array_type" => {
                let element_node = node.child_by_field_name("element")
                    .or_else(|| node.child(0));
                
                let element = if let Some(e) = element_node {
                    self.parse_type_node(e, source)
                } else {
                    TypeRef::Unknown
                };

                let dim_node = node.child_by_field_name("dimensions");
                let count = if let Some(d) = dim_node {
                     d.utf8_text(source.as_bytes()).unwrap_or("").matches('[').count()
                } else {
                    1
                };

                TypeRef::Array {
                    element: Box::new(element),
                    dimensions: count,
                }
            },
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
            },
            _ => {
                let text = node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                if text.is_empty() {
                    TypeRef::Unknown
                } else {
                    TypeRef::Raw(text)
                }
            }
        }
    }

    // --- Core Atomic Helpers (Shared between Global and Local) ---

    /// Gets the full FQN for a definition node.
    pub fn get_fqn_for_definition(&self, name_node: &Node, source: &str, pkg: Option<&str>) -> String {
        let mut parts = Vec::new();
        let mut curr = *name_node;
        let mut seen_ids = std::collections::HashSet::new();

        parts.push(name_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string());
        seen_ids.insert(name_node.id());

        while let Some(parent) = self.find_next_enclosing_definition(curr) {
            let kind = parent.kind();
            // FQN for Java elements should only include Packages and Classes/Interfaces/Enums.
            // Methods and Constructors should be skipped when calculating the FQN of nested elements.
            if kind.contains("class") || kind.contains("interface") || kind.contains("enum") || kind.contains("annotation") {
                if let Some(n_node) = parent.child_by_field_name("name") {
                    if seen_ids.insert(n_node.id()) {
                        if let Ok(n_text) = n_node.utf8_text(source.as_bytes()) {
                            parts.push(n_text.to_string());
                        }
                    }
                }
            }
            curr = parent;
        }
        
        parts.reverse();
        let mut fqn = if let Some(p) = pkg { p.to_string() } else { String::new() };
        for p in parts {
            if !fqn.is_empty() { fqn.push('.'); }
            fqn.push_str(&p);
        }
        fqn
    }

    /// Returns a list of FQNs for all enclosing classes from inner to outer.
    pub fn get_enclosing_class_fqns(&self, node: &Node, source: &str, pkg: Option<&str>) -> Vec<String> {
        let mut fqns = Vec::new();
        let mut curr = *node;

        // If the current node is the name of a definition, start searching from the definition itself
        // to avoid including the current definition in the enclosing class list.
        if let Some(parent) = curr.parent() {
            if Self::is_definition_node(parent.kind()) {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == node.id() {
                        curr = parent;
                    }
                }
            }
        }

        while let Some(container) = self.find_next_enclosing_definition(curr) {
            let kind = container.kind();
            if kind.contains("class") || kind.contains("interface") || kind.contains("enum") {
                if let Some(name_node) = container.child_by_field_name("name") {
                    fqns.push(self.get_fqn_for_definition(&name_node, source, pkg));
                }
            }
            curr = container;
        }
        fqns
    }

    pub fn find_local_declaration(&self, start_node: Node, name: &str, source: &str) -> Option<(Range, Option<String>)> {
        let mut curr = start_node;
        while let Some(parent) = curr.parent() {
            // Check declarations in this scope before the start_node
            let mut child_cursor = parent.walk();
            for child in parent.children(&mut child_cursor) {
                if child.start_byte() >= start_node.start_byte() {
                    break;
                }
                if let Some(res) = self.is_decl_of(&child, name, source) {
                    return Some(res);
                }
            }
            // Check if parent itself is a declaration (like method parameters)
            if let Some(res) = self.is_decl_of(&parent, name, source) {
                return Some(res);
            }
            curr = parent;
        }
        None
    }

    pub fn extract_package_and_imports(&self, tree: &Tree, source: &str) -> (Option<String>, Vec<String>) {
        let mut package = None;
        let mut imports = Vec::new();
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(&self.definition_query, tree.root_node(), source.as_bytes());
        while let Some(mat) = matches.next() {
            if let Some(cap) = mat.captures.iter().find(|c| c.index == self.indices.pkg) {
                package = cap.node.utf8_text(source.as_bytes()).ok().map(|s: &str| s.to_string());
            } else if let Some(cap) = mat.captures.iter().find(|c| c.index == self.indices.import_name) {
                if let Ok(imp) = cap.node.utf8_text(source.as_bytes()) {
                    let imp_str: &str = imp;
                    imports.push(imp_str.to_string());
                }
            }
        }
        (package, imports)
    }

    pub fn resolve_type_name_to_fqn_data(&self, type_name: &str, package: Option<&str>, imports: &[String]) -> Option<String> {
        // 1. Check if it's a primitive type
        const PRIMITIVES: &[&str] = &["int", "long", "short", "byte", "float", "double", "boolean", "char", "void"];
        if PRIMITIVES.contains(&type_name) {
            return Some(type_name.to_string());
        }

        // 2. Already an FQN?
        if type_name.contains('.') {
            return Some(type_name.to_string());
        }

        // 3. Precise imports
        for imp in imports {
            if imp.ends_with(&format!(".{}", type_name)) {
                return Some(imp.clone());
            }
        }

        // 4. Wildcard imports (e.g., import java.util.*;)
        // Note: This is heuristic-lite but necessary for correctness in Java
        // We'll return the first one that might match, or wait for index resolution?
        // Actually, without a full classpath, we can only guess. 
        // For now, let's focus on java.lang which is always there.

        // 5. java.lang (implicit import)
        // List of common java.lang classes to avoid false positives? 
        // Or just assume if not found elsewhere, it might be java.lang
        const JAVA_LANG_CLASSES: &[&str] = &[
            "String", "Object", "Integer", "Long", "Double", "Float", "Boolean", "Byte", "Character", "Short",
            "Exception", "RuntimeException", "Throwable", "Error", "Thread", "System", "Class", "Iterable",
            "Runnable", "Comparable", "SuppressWarnings", "Override", "Deprecated"
        ];
        if JAVA_LANG_CLASSES.contains(&type_name) {
            return Some(format!("java.lang.{}", type_name));
        }

        // 6. Current package
        if let Some(p) = package {
            return Some(format!("{}.{}", p, type_name));
        }

        Some(type_name.to_string())
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
            "class_declaration" | "interface_declaration" | "enum_declaration" | "annotation_type_declaration" => SymbolIntent::Type,
            "method_declaration" | "constructor_declaration" => {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == node.id() {
                        return SymbolIntent::Method;
                    }
                }
                SymbolIntent::Type
            },
            _ => {
                if node.kind() == "type_identifier" || node.kind() == "scoped_type_identifier" {
                    SymbolIntent::Type
                } else {
                    SymbolIntent::Unknown
                }
            }
        }
    }

    pub fn is_decl_of(&self, node: &Node, name: &str, source: &str) -> Option<(Range, Option<String>)> {
        match node.kind() {
            "variable_declarator" | "formal_parameter" | "catch_formal_parameter" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source.as_bytes()).ok()? == name {
                        let range = range_from_ts(name_node.range());
                        let type_node = if node.kind() == "variable_declarator" {
                            // Type is in the parent local_variable_declaration
                            node.parent()
                                .and_then(|p| p.child_by_field_name("type"))
                        } else {
                            // Type is a sibling for parameters
                            node.child_by_field_name("type")
                        };
                        let type_name = type_node.and_then(|t| t.utf8_text(source.as_bytes()).ok().map(|s| s.to_string()));
                        return Some((range, type_name));
                    }
                }
            }
            "local_variable_declaration" | "formal_parameters" | "inferred_parameters" | "enhanced_for_statement" | "lambda_expression" => {
                if node.kind() == "lambda_expression" {
                    if let Some(params) = node.child_by_field_name("parameters") {
                        if params.kind() == "identifier" {
                            if params.utf8_text(source.as_bytes()).ok()? == name {
                                return Some((range_from_ts(params.range()), None));
                            }
                        } else {
                            return self.is_decl_of(&params, name, source);
                        }
                    }
                    return None;
                }
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(res) = self.is_decl_of(&child, name, source) { return Some(res); }
                }
            }
            _ => {}
        }
        None
    }

    pub fn resolve_type_name_to_fqn(&self, type_name: &str, tree: &Tree, source: &str) -> Option<String> {
        let (pkg, imports) = self.extract_package_and_imports(tree, source);
        self.resolve_type_name_to_fqn_data(type_name, pkg.as_deref(), &imports)
    }

    // --- Private Helpers ---

    fn is_definition_node(kind: &str) -> bool {
        matches!(
            kind,
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration"
                | "method_declaration"
                | "constructor_declaration"
        )
    }

    pub fn find_next_enclosing_definition<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            if Self::is_definition_node(parent.kind()) {
                return Some(parent);
            }
            curr = parent;
        }
        None
    }
}
