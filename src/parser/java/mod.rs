use crate::error::{NaviscopeError, Result};
use crate::model::graph::{EdgeType, GraphNode, Range};
use crate::model::lang::java::{
    JavaClass, JavaElement, JavaField, JavaInterface, JavaMethod, JavaParameter,
};
use std::collections::HashMap;
use tree_sitter::{Parser, Query, QueryCursor, QueryMatch, StreamingIterator};

mod constants;
use constants::*;

unsafe extern "C" {
    fn tree_sitter_java() -> tree_sitter::Language;
}

use crate::parser::queries::java_definitions::JavaIndices;

pub struct JavaParser {
    language: tree_sitter::Language,
    definition_query: Query,
    indices: JavaIndices,
}

pub struct JavaParseResult {
    pub package_name: Option<String>,
    pub imports: Vec<String>,
    pub nodes: Vec<GraphNode>,
    pub relations: Vec<(String, String, EdgeType, Option<Range>)>,
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

    pub fn parse_file(&self, source_code: &str, file_path: Option<&std::path::Path>) -> Result<JavaParseResult> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;

        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| NaviscopeError::Parsing("Failed to parse Java file".to_string()))?;

        let mut package_name = String::new();
        let mut imports = Vec::new();
        let mut elements_map: HashMap<String, JavaElement> = HashMap::new();
        let mut relations = Vec::new();

        let mut cursor = QueryCursor::new();
        let matches = cursor.matches(
            &self.definition_query,
            tree.root_node(),
            source_code.as_bytes(),
        );
        let mut matches = matches;

        while let Some(mat) = matches.next() {
            self.process_match(
                mat,
                source_code,
                &self.indices,
                &mut package_name,
                &mut imports,
                &mut elements_map,
                &mut relations,
            );
        }

        let nodes = elements_map
            .into_values()
            .map(|e| GraphNode::java(e, file_path.map(|p| p.to_path_buf())))
            .collect();

        Ok(JavaParseResult {
            package_name: if package_name.is_empty() {
                None
            } else {
                Some(package_name)
            },
            imports,
            nodes,
            relations,
        })
    }

    fn process_match(
        &self,
        mat: &QueryMatch,
        source: &str,
        idx: &JavaIndices,
        pkg_name: &mut String,
        imports: &mut Vec<String>,
        elements: &mut HashMap<String, JavaElement>,
        relations: &mut Vec<(String, String, EdgeType, Option<Range>)>,
    ) {
        if let Some(cap) = mat.captures.iter().find(|c| c.index == idx.pkg) {
            *pkg_name = cap
                .node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .to_string();
            return;
        }

        if let Some(cap) = mat.captures.iter().find(|c| c.index == idx.import_name) {
            let imp = cap
                .node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .to_string();
            imports.push(imp);
            return;
        }

        let anchor = mat.captures.iter().find(|c| {
            let i = c.index;
            i == idx.class_def
                || i == idx.inter_def
                || i == idx.method_def
                || i == idx.constr_def
                || i == idx.field_def
                || i == idx.method_param_match
                || i == idx.constr_param_match
        });

        if let Some(anchor) = anchor {
            self.handle_definition(mat, source, idx, pkg_name, elements, relations, anchor);
            return;
        }

        if let Some(call_cap) = mat.captures.iter().find(|c| c.index == idx.call) {
            if let (Some(target_node), Some(source_fqn)) = (
                mat.captures
                    .iter()
                    .find(|c| c.index == idx.call_name)
                    .map(|c| c.node),
                self.find_enclosing_element(call_cap.node, source, pkg_name),
            ) {
                let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                let r = target_node.range();
                let range = Range {
                    start_line: r.start_point.row,
                    start_col: r.start_point.column,
                    end_line: r.end_point.row,
                    end_col: r.end_point.column,
                };
                relations.push((source_fqn, target, EdgeType::Calls, Some(range)));
            }
        } else if let Some(inst_cap) = mat.captures.iter().find(|c| c.index == idx.inst) {
            if let (Some(target_node), Some(source_fqn)) = (
                mat.captures
                    .iter()
                    .find(|c| c.index == idx.inst_type)
                    .map(|c| c.node),
                self.find_enclosing_element(inst_cap.node, source, pkg_name),
            ) {
                let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                let r = target_node.range();
                let range = Range {
                    start_line: r.start_point.row,
                    start_col: r.start_point.column,
                    end_line: r.end_point.row,
                    end_col: r.end_point.column,
                };
                relations.push((source_fqn, target, EdgeType::Instantiates, Some(range)));
            }
        }
    }

    fn handle_definition(
        &self,
        mat: &QueryMatch,
        source: &str,
        idx: &JavaIndices,
        pkg: &str,
        elements: &mut HashMap<String, JavaElement>,
        relations: &mut Vec<(String, String, EdgeType, Option<Range>)>,
        anchor: &tree_sitter::QueryCapture,
    ) {
        let kind = if anchor.index == idx.class_def {
            KIND_LABEL_CLASS
        } else if anchor.index == idx.inter_def {
            KIND_LABEL_INTERFACE
        } else if anchor.index == idx.method_def || anchor.index == idx.method_param_match {
            KIND_LABEL_METHOD
        } else if anchor.index == idx.constr_def || anchor.index == idx.constr_param_match {
            KIND_LABEL_CONSTRUCTOR
        } else {
            KIND_LABEL_FIELD
        };

        let name_idx = match kind {
            KIND_LABEL_CLASS => idx.class_name,
            KIND_LABEL_INTERFACE => idx.inter_name,
            KIND_LABEL_METHOD => idx.method_name,
            KIND_LABEL_CONSTRUCTOR => idx.constr_name,
            KIND_LABEL_FIELD => idx.field_name,
            _ => 0,
        };

        if let Some(name_node) = mat
            .captures
            .iter()
            .find(|c| c.index == name_idx)
            .map(|c| c.node)
        {
            let fqn = self.compute_fqn(name_node, source, pkg);
            let name = name_node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .to_string();

            let node_range = anchor.node.range();
            let range = Some(Range {
                start_line: node_range.start_point.row,
                start_col: node_range.start_point.column,
                end_line: node_range.end_point.row,
                end_col: node_range.end_point.column,
            });

            let name_ts_range = name_node.range();
            let name_range = Some(Range {
                start_line: name_ts_range.start_point.row,
                start_col: name_ts_range.start_point.column,
                end_line: name_ts_range.end_point.row,
                end_col: name_ts_range.end_point.column,
            });

            let entry = elements.entry(fqn.clone()).or_insert_with(|| {
                let e = match kind {
                    KIND_LABEL_CLASS => JavaElement::Class(JavaClass {
                        id: fqn.clone(),
                        name: name.clone(),
                        modifiers: vec![],
                        superclass: None,
                        interfaces: vec![],
                        range: range.clone(),
                        name_range: name_range.clone(),
                    }),
                    KIND_LABEL_INTERFACE => JavaElement::Interface(JavaInterface {
                        id: fqn.clone(),
                        name: name.clone(),
                        modifiers: vec![],
                        extends: vec![],
                        range: range.clone(),
                        name_range: name_range.clone(),
                    }),
                    KIND_LABEL_METHOD | KIND_LABEL_CONSTRUCTOR => JavaElement::Method(JavaMethod {
                        id: fqn.clone(),
                        name: name.clone(),
                        return_type: "void".to_string(),
                        parameters: vec![],
                        modifiers: vec![],
                        is_constructor: kind == KIND_LABEL_CONSTRUCTOR,
                        range: range.clone(),
                        name_range: name_range.clone(),
                    }),
                    KIND_LABEL_FIELD => JavaElement::Field(JavaField {
                        id: fqn.clone(),
                        name: name.clone(),
                        type_name: "".to_string(),
                        modifiers: vec![],
                        range: range.clone(),
                        name_range: name_range.clone(),
                    }),
                    _ => unreachable!(),
                };
                if let Some(parent) = self.find_enclosing_element(anchor.node, source, pkg) {
                    if parent != fqn {
                        relations.push((parent, fqn.clone(), EdgeType::Contains, None));
                    }
                }
                e
            });

            self.merge_metadata(mat, source, idx, &fqn, entry, relations);
        }
    }

    fn merge_metadata(
        &self,
        mat: &QueryMatch,
        source: &str,
        idx: &JavaIndices,
        fqn: &str,
        element: &mut JavaElement,
        relations: &mut Vec<(String, String, EdgeType, Option<Range>)>,
    ) {
        for cap in mat.captures.iter().filter(|c| c.index == idx.mods) {
            let m = cap
                .node
                .utf8_text(source.as_bytes())
                .unwrap_or("")
                .to_string();
            match element {
                JavaElement::Class(c) => {
                    if !c.modifiers.contains(&m) {
                        c.modifiers.push(m);
                    }
                }
                JavaElement::Interface(i) => {
                    if !i.modifiers.contains(&m) {
                        i.modifiers.push(m);
                    }
                }
                JavaElement::Method(m_node) => {
                    if !m_node.modifiers.contains(&m) {
                        m_node.modifiers.push(m);
                    }
                }
                JavaElement::Field(f) => {
                    if !f.modifiers.contains(&m) {
                        f.modifiers.push(m);
                    }
                }
                _ => {}
            }
        }

        match element {
            JavaElement::Class(c) => {
                if let Some(s) = mat
                    .captures
                    .iter()
                    .find(|c| c.index == idx.class_super)
                    .map(|c| {
                        c.node
                            .utf8_text(source.as_bytes())
                            .unwrap_or("")
                            .to_string()
                    })
                {
                    c.superclass = Some(s.clone());
                    relations.push((fqn.to_string(), s, EdgeType::InheritsFrom, None));
                }
                for cc in mat.captures.iter().filter(|c| c.index == idx.class_inter) {
                    let i = cc
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    if !c.interfaces.contains(&i) {
                        c.interfaces.push(i.clone());
                        relations.push((fqn.to_string(), i, EdgeType::Implements, None));
                    }
                }
            }
            JavaElement::Interface(i) => {
                for cc in mat.captures.iter().filter(|c| c.index == idx.inter_ext) {
                    let e = cc
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    if !i.extends.contains(&e) {
                        i.extends.push(e.clone());
                        relations.push((fqn.to_string(), e, EdgeType::InheritsFrom, None));
                    }
                }
            }
            JavaElement::Method(m) => {
                if let Some(ret) = mat.captures.iter().find(|c| c.index == idx.method_ret) {
                    m.return_type = ret
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                }
                if let (Some(t_node), Some(n_node)) = (
                    mat.captures
                        .iter()
                        .find(|c| c.index == idx.param_type)
                        .map(|c| c.node),
                    mat.captures
                        .iter()
                        .find(|c| c.index == idx.param_name)
                        .map(|c| c.node),
                ) {
                    let t = t_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    let n = n_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                    if !m.parameters.iter().any(|p| p.name == n && p.type_name == t) {
                        m.parameters.push(JavaParameter {
                            type_name: t,
                            name: n,
                        });
                    }
                }
            }
            JavaElement::Field(f) => {
                if let Some(t) = mat.captures.iter().find(|c| c.index == idx.field_type) {
                    f.type_name = t
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or("")
                        .to_string();
                }
            }
            _ => {}
        }
    }

    fn find_enclosing_element(
        &self,
        node: tree_sitter::Node,
        source: &str,
        pkg: &str,
    ) -> Option<String> {
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            match parent.kind() {
                KIND_CLASS_DECL | KIND_INTERFACE_DECL | KIND_ENUM_DECL | KIND_METHOD_DECL
                | KIND_CONSTR_DECL => {
                    if let Some(name_node) = parent.child_by_field_name("name") {
                        let fqn = self.compute_fqn(name_node, source, pkg);
                        if let Some(own_name_node) = node.child_by_field_name("name") {
                            if let Ok(own_text) = own_name_node.utf8_text(source.as_bytes()) {
                                if !fqn.ends_with(own_text) {
                                    return Some(fqn);
                                }
                            }
                        } else {
                            return Some(fqn);
                        }
                    }
                }
                _ => {}
            }
            curr = parent;
        }
        None
    }

    fn compute_fqn(
        &self,
        name_node: tree_sitter::Node,
        source_code: &str,
        package_name: &str,
    ) -> String {
        let mut parts = Vec::new();
        let mut curr = name_node;

        // Push the name of the entity itself
        parts.push(
            name_node
                .utf8_text(source_code.as_bytes())
                .unwrap_or("")
                .to_string(),
        );

        // Move to the parent of the declaration that contains this name node
        if let Some(decl) = curr.parent() {
            curr = decl;
        }

        while let Some(parent) = curr.parent() {
            match parent.kind() {
                KIND_CLASS_DECL | KIND_INTERFACE_DECL | KIND_ENUM_DECL | KIND_METHOD_DECL
                | KIND_CONSTR_DECL => {
                    if let Some(n_node) = parent.child_by_field_name("name") {
                        let text = n_node
                            .utf8_text(source_code.as_bytes())
                            .unwrap_or("")
                            .to_string();
                        parts.push(text);
                    }
                }
                _ => {}
            }
            curr = parent;
        }
        parts.reverse();
        let mut fqn = if package_name.is_empty() {
            String::new()
        } else {
            package_name.to_string()
        };
        for p in parts {
            if !fqn.is_empty() {
                fqn.push('.');
            }
            fqn.push_str(&p);
        }
        fqn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_java_full() {
        let source = r#"
            package com.example;
            public class Test extends Base implements Sync {
                private String field1;
                public void method1(int a, String b) {
                    otherMethod();
                    new OtherClass();
                }
                public static class Inner {
                    void innerMethod() {}
                }
            }
            interface Sync {}
            class Base {}
            class OtherClass {}
        "#;
        let parser = JavaParser::new().unwrap();
        let result = parser.parse_file(source, Some(std::path::Path::new("Test.java"))).unwrap();

        let mut ids: Vec<_> = result.nodes.iter().map(|e| e.fqn().to_string()).collect();
        ids.sort();

        assert!(ids.contains(&"com.example.Test".to_string()));
        assert!(ids.contains(&"com.example.Test.method1".to_string()));

        let rels = result.relations;
        assert!(rels.iter().any(|(f, t, e, _)| f == "com.example.Test"
            && t == "com.example.Test.method1"
            && *e == EdgeType::Contains));

        let m1 = result
            .nodes
            .iter()
            .find(|e| e.fqn() == "com.example.Test.method1")
            .unwrap();
        
        match m1 {
            GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Method(m), .. }) => {
                assert_eq!(m.modifiers.contains(&"public".to_string()), true);
                assert_eq!(m.parameters.len(), 2);
            }
            _ => panic!("Expected Java method node"),
        }
    }
}
