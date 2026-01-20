use crate::error::Result;
use crate::model::graph::Range;
use tree_sitter::{Node, Query, Tree, StreamingIterator};

mod constants;
mod lsp;
mod index;
mod ast;
use constants::*;

unsafe extern "C" {
    fn tree_sitter_java() -> tree_sitter::Language;
}

use crate::parser::queries::java_definitions::JavaIndices;
use crate::parser::SymbolIntent;

pub struct JavaParser {
    pub(crate) language: tree_sitter::Language,
    pub(crate) definition_query: Query,
    pub(crate) indices: JavaIndices,
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
            definition_query,
            indices,
        })
    }

    // --- Core Atomic Helpers (Shared between Global and Local) ---

    pub(crate) fn compute_fqn(&self, name_node: Node, source: &str, package: &str) -> String {
        let mut parts = Vec::new();
        let mut curr = name_node;
        parts.push(name_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string());
        if let Some(decl) = curr.parent() { curr = decl; }
        while let Some(parent) = curr.parent() {
            match parent.kind() {
                KIND_CLASS_DECL | KIND_INTERFACE_DECL | KIND_ENUM_DECL | KIND_METHOD_DECL | KIND_CONSTR_DECL => {
                    if let Some(n_node) = parent.child_by_field_name("name") {
                        parts.push(n_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string());
                    }
                }
                _ => {}
            }
            curr = parent;
        }
        parts.reverse();
        let mut fqn = if package.is_empty() { String::new() } else { package.to_string() };
        for p in parts {
            if !fqn.is_empty() { fqn.push('.'); }
            fqn.push_str(&p);
        }
        fqn
    }

    pub(crate) fn find_enclosing_element(&self, node: Node, source: &str, pkg: &str) -> Option<String> {
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            match parent.kind() {
                KIND_CLASS_DECL | KIND_INTERFACE_DECL | KIND_ENUM_DECL | KIND_METHOD_DECL | KIND_CONSTR_DECL => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        let fqn = self.compute_fqn(name_node, source, pkg);
                        if let Some(own_name_node) = node.child_by_field_name("name") {
                            if let Ok(own_text) = own_name_node.utf8_text(source.as_bytes()) {
                                if !fqn.ends_with(own_text) { return Some(fqn); }
                            }
                        } else { return Some(fqn); }
                    }
                }
                _ => {}
            }
            curr = parent;
        }
        None
    }

    pub(crate) fn extract_package_and_imports(&self, tree: &Tree, source: &str) -> (Option<String>, Vec<String>) {
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

    pub(crate) fn determine_intent(&self, node: &Node) -> SymbolIntent {
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
            _ => {
                if node.kind() == "type_identifier" || node.kind() == "scoped_type_identifier" {
                    SymbolIntent::Type
                } else {
                    SymbolIntent::Unknown
                }
            }
        }
    }

    pub(crate) fn is_decl_of(&self, node: &Node, name: &str, source: &str) -> Option<Range> {
        match node.kind() {
            "variable_declarator" | "formal_parameter" | "catch_formal_parameter" => {
                if let Some(name_node) = node.child_by_field_name("name") {
                    if name_node.utf8_text(source.as_bytes()).ok()? == name {
                        return Some(Range::from_ts(name_node.range()));
                    }
                }
            }
            "local_variable_declaration" | "formal_parameters" | "inferred_parameters" | "enhanced_for_statement" => {
                let mut cursor = node.walk();
                for child in node.children(&mut cursor) {
                    if let Some(r) = self.is_decl_of(&child, name, source) { return Some(r); }
                }
            }
            _ => {}
        }
        None
    }

    pub(crate) fn resolve_receiver_type(&self, receiver: &Node, tree: &Tree, source: &str) -> Option<String> {
        let receiver_text = receiver.utf8_text(source.as_bytes()).ok()?;

        // Handle 'this' keyword
        if receiver_text == "this" {
            let (pkg, _) = self.extract_package_and_imports(tree, source);
            return self.find_enclosing_class_fqn(receiver, source, pkg.as_deref());
        }

        let mut curr = *receiver;
        while let Some(parent) = curr.parent() {
            let mut cursor = parent.walk();
            for child in parent.children(&mut cursor) {
                if child.start_byte() >= receiver.start_byte() { break; }
                if child.kind() == "local_variable_declaration" {
                    if let Some(type_node) = child.child_by_field_name("type") {
                        let mut vd_cursor = child.walk();
                        for vd in child.children(&mut vd_cursor) {
                            if vd.kind() == "variable_declarator" {
                                if let Some(name_node) = vd.child_by_field_name("name") {
                                    if name_node.utf8_text(source.as_bytes()).ok()? == receiver_text {
                                        let type_name = type_node.utf8_text(source.as_bytes()).ok()?;
                                        return self.resolve_type_name_to_fqn(type_name, tree, source);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            curr = parent;
        }
        None
    }

    pub(crate) fn resolve_type_name_to_fqn(&self, type_name: &str, tree: &Tree, source: &str) -> Option<String> {
        let (pkg, imports) = self.extract_package_and_imports(tree, source);
        for imp in &imports {
            if imp.ends_with(&format!(".{}", type_name)) { return Some(imp.clone()); }
        }
        if let Some(p) = pkg { return Some(format!("{}.{}", p, type_name)); }
        Some(type_name.to_string())
    }

    pub(crate) fn find_enclosing_class_fqn(&self, node: &Node, source: &str, pkg: Option<&str>) -> Option<String> {
        let mut curr = *node;
        while let Some(parent) = curr.parent() {
            if parent.kind() == "class_declaration" {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    let name = name_node.utf8_text(source.as_bytes()).ok()?;
                    return Some(if let Some(p) = pkg { format!("{}.{}", p, name) } else { name.to_string() });
                }
            }
            curr = parent;
        }
        None
    }
}
