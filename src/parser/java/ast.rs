use crate::model::graph::{EdgeType, Range};
use crate::model::lang::java::{
    JavaAnnotation, JavaClass, JavaElement, JavaEnum, JavaField, JavaInterface, JavaMethod,
    JavaParameter,
};
use tree_sitter::{Node, QueryMatch, Tree, StreamingIterator};
use super::JavaParser;
use super::constants::*;

/// The native semantic model of a Java source file.
pub struct JavaFileModel<'a> {
    pub package: Option<String>,
    pub imports: Vec<String>,
    pub entities: Vec<JavaEntity<'a>>,
    pub relations: Vec<JavaRelation>,
}

pub struct JavaEntity<'a> {
    pub element: JavaElement,
    pub node: Node<'a>,
}

pub struct JavaRelation {
    pub source_fqn: String,
    pub target_name: String,
    pub rel_type: EdgeType,
    pub range: Option<Range>,
}

impl JavaParser {
    /// Deeply analyzes a Java tree and produces a native JavaFileModel.
    pub(crate) fn analyze<'a>(&self, tree: &'a Tree, source: &'a str) -> JavaFileModel<'a> {
        let (package, imports) = self.extract_package_and_imports(tree, source);
        let mut entities = Vec::new();
        let mut relations = Vec::new();

        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches = cursor.matches(
            &self.definition_query,
            tree.root_node(),
            source.as_bytes(),
        );

        let pkg_str = package.as_deref().unwrap_or("");
        let mut entities_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        while let Some(mat) = matches.next() {
            // Find definitions
            let anchor = mat.captures.iter().find(|c| {
                let i = c.index;
                i == self.indices.class_def
                    || i == self.indices.inter_def
                    || i == self.indices.enum_def
                    || i == self.indices.annotation_def
                    || i == self.indices.method_def
                    || i == self.indices.constr_def
                    || i == self.indices.field_def
                    || i == self.indices.method_param_match
                    || i == self.indices.constr_param_match
            });

            if let Some(anchor) = anchor {
                let anchor_node = anchor.node;
                let (kind, name_idx) = if anchor.index == self.indices.class_def {
                    (KIND_LABEL_CLASS, self.indices.class_name)
                } else if anchor.index == self.indices.inter_def {
                    (KIND_LABEL_INTERFACE, self.indices.inter_name)
                } else if anchor.index == self.indices.enum_def {
                    (KIND_LABEL_ENUM, self.indices.enum_name)
                } else if anchor.index == self.indices.annotation_def {
                    (KIND_LABEL_ANNOTATION, self.indices.annotation_name)
                } else if anchor.index == self.indices.method_def || anchor.index == self.indices.method_param_match {
                    (KIND_LABEL_METHOD, self.indices.method_name)
                } else if anchor.index == self.indices.constr_def || anchor.index == self.indices.constr_param_match {
                    (KIND_LABEL_CONSTRUCTOR, self.indices.constr_name)
                } else {
                    (KIND_LABEL_FIELD, self.indices.field_name)
                };

                if let Some(name_node) = mat.captures.iter().find(|c| c.index == name_idx).map(|c| c.node) {
                    let fqn = self.compute_fqn(name_node, source, pkg_str);
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    let range = Range::from_ts(anchor_node.range());
                    let name_range = Range::from_ts(name_node.range());

                    let idx = if let Some(&existing_idx) = entities_map.get(&fqn) {
                        existing_idx
                    } else {
                        let new_idx = entities.len();
                        let element = match kind {
                            KIND_LABEL_CLASS => JavaElement::Class(JavaClass {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                superclass: None, interfaces: vec![], range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_INTERFACE => JavaElement::Interface(JavaInterface {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                extends: vec![], range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_ENUM => JavaElement::Enum(JavaEnum {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                interfaces: vec![], constants: vec![], range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_ANNOTATION => JavaElement::Annotation(JavaAnnotation {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_METHOD | KIND_LABEL_CONSTRUCTOR => JavaElement::Method(JavaMethod {
                                id: fqn.clone(), name: name.clone(), return_type: "void".to_string(),
                                parameters: vec![], modifiers: vec![], is_constructor: kind == KIND_LABEL_CONSTRUCTOR,
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_FIELD => JavaElement::Field(JavaField {
                                id: fqn.clone(), name: name.clone(), type_name: "".to_string(),
                                modifiers: vec![], range: Some(range), name_range: Some(name_range),
                            }),
                            _ => unreachable!(),
                        };
                        entities.push(JavaEntity {
                            element,
                            node: anchor_node,
                        });
                        entities_map.insert(fqn.clone(), new_idx);

                        // Structural relation
                        if let Some(parent) = self.find_enclosing_element(anchor_node, source, pkg_str) {
                            if parent != fqn {
                                relations.push(JavaRelation {
                                    source_fqn: parent,
                                    target_name: fqn.clone(),
                                    rel_type: EdgeType::Contains,
                                    range: None,
                                });
                            }
                        }
                        new_idx
                    };

                    self.merge_metadata_to_model(mat, source, fqn, &mut entities[idx].element, &mut relations);
                }
                continue;
            }

            // Call/Instantiates relations
            if let Some(call_cap) = mat.captures.iter().find(|c| c.index == self.indices.call) {
                if let (Some(target_node), Some(source_fqn)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.call_name).map(|c| c.node),
                    self.find_enclosing_element(call_cap.node, source, pkg_str),
                ) {
                    let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    relations.push(JavaRelation {
                        source_fqn,
                        target_name: target,
                        rel_type: EdgeType::Calls,
                        range: Some(Range::from_ts(target_node.range())),
                    });
                }
            } else if let Some(inst_cap) = mat.captures.iter().find(|c| c.index == self.indices.inst) {
                if let (Some(target_node), Some(source_fqn)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.inst_type).map(|c| c.node),
                    self.find_enclosing_element(inst_cap.node, source, pkg_str),
                ) {
                    let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                    relations.push(JavaRelation {
                        source_fqn,
                        target_name: target,
                        rel_type: EdgeType::Instantiates,
                        range: Some(Range::from_ts(target_node.range())),
                    });
                }
            }
        }

        JavaFileModel {
            package,
            imports,
            entities,
            relations,
        }
    }

    fn merge_metadata_to_model(
        &self,
        mat: &QueryMatch,
        source: &str,
        fqn: String,
        element: &mut JavaElement,
        relations: &mut Vec<JavaRelation>,
    ) {
        for cap in mat.captures.iter().filter(|c| c.index == self.indices.mods) {
            let m = cap.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
            match element {
                JavaElement::Class(c) => if !c.modifiers.contains(&m) { c.modifiers.push(m); }
                JavaElement::Interface(i) => if !i.modifiers.contains(&m) { i.modifiers.push(m); }
                JavaElement::Enum(e) => if !e.modifiers.contains(&m) { e.modifiers.push(m); }
                JavaElement::Annotation(a) => if !a.modifiers.contains(&m) { a.modifiers.push(m); }
                JavaElement::Method(m_node) => if !m_node.modifiers.contains(&m) { m_node.modifiers.push(m); }
                JavaElement::Field(f) => if !f.modifiers.contains(&m) { f.modifiers.push(m); }
            }
        }

        match element {
            JavaElement::Class(c) => {
                if let Some(s) = mat.captures.iter().find(|c| c.index == self.indices.class_super) {
                    let s_name = s.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    c.superclass = Some(s_name.clone());
                    relations.push(JavaRelation {
                        source_fqn: fqn.clone(),
                        target_name: s_name,
                        rel_type: EdgeType::InheritsFrom,
                        range: None,
                    });
                }
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.class_inter) {
                    let i = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !c.interfaces.contains(&i) {
                        c.interfaces.push(i.clone());
                        relations.push(JavaRelation {
                            source_fqn: fqn.clone(),
                            target_name: i,
                            rel_type: EdgeType::Implements,
                            range: None,
                        });
                    }
                }
            }
            JavaElement::Interface(i) => {
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.inter_ext) {
                    let e = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !i.extends.contains(&e) {
                        i.extends.push(e.clone());
                        relations.push(JavaRelation {
                            source_fqn: fqn.clone(),
                            target_name: e,
                            rel_type: EdgeType::InheritsFrom,
                            range: None,
                        });
                    }
                }
            }
            JavaElement::Enum(e) => {
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.enum_interface) {
                    let i = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !e.interfaces.contains(&i) {
                        e.interfaces.push(i.clone());
                        relations.push(JavaRelation {
                            source_fqn: fqn.clone(),
                            target_name: i,
                            rel_type: EdgeType::Implements,
                            range: None,
                        });
                    }
                }
                if let Some(const_cap) = mat.captures.iter().find(|c| c.index == self.indices.enum_constant) {
                    let c_name = const_cap.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !e.constants.contains(&c_name) {
                        e.constants.push(c_name);
                    }
                }
            }
            JavaElement::Annotation(_) => {}
            JavaElement::Method(m) => {
                if let Some(ret) = mat.captures.iter().find(|c| c.index == self.indices.method_ret) {
                    m.return_type = ret.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                }
                if let (Some(t_node), Some(n_node)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.param_type).map(|c| c.node),
                    mat.captures.iter().find(|c| c.index == self.indices.param_name).map(|c| c.node),
                ) {
                    let t = t_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    let n = n_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !m.parameters.iter().any(|p| p.name == n && p.type_name == t) {
                        m.parameters.push(JavaParameter { type_name: t, name: n });
                    }
                }
            }
            JavaElement::Field(f) => {
                if let Some(t) = mat.captures.iter().find(|c| c.index == self.indices.field_type) {
                    f.type_name = t.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                }
            }
        }
    }
}
