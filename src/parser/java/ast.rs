use crate::model::graph::{EdgeType, Range};
use crate::parser::utils::range_from_ts;
use crate::model::lang::java::{
    JavaAnnotation, JavaClass, JavaElement, JavaEnum, JavaField, JavaInterface, JavaMethod,
    JavaParameter,
};
use crate::model::signature::TypeRef;
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

        let mut entities_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

        while let Some(mat) = matches.next() {
            // 1. Find definitions (Anchors)
            let definition_anchor = mat.captures.iter().find(|c| {
                let i = c.index;
                i == self.indices.class_def
                    || i == self.indices.inter_def
                    || i == self.indices.enum_def
                    || i == self.indices.annotation_def
                    || i == self.indices.method_def
                    || i == self.indices.constr_def
                    || i == self.indices.field_def
            });

            if let Some(anchor) = definition_anchor {
                let anchor_node = anchor.node;
                let (kind, name_idx) = if anchor.index == self.indices.class_def {
                    (KIND_LABEL_CLASS, self.indices.class_name)
                } else if anchor.index == self.indices.inter_def {
                    (KIND_LABEL_INTERFACE, self.indices.inter_name)
                } else if anchor.index == self.indices.enum_def {
                    (KIND_LABEL_ENUM, self.indices.enum_name)
                } else if anchor.index == self.indices.annotation_def {
                    (KIND_LABEL_ANNOTATION, self.indices.annotation_name)
                } else if anchor.index == self.indices.method_def {
                    (KIND_LABEL_METHOD, self.indices.method_name)
                } else if anchor.index == self.indices.constr_def {
                    (KIND_LABEL_CONSTRUCTOR, self.indices.constr_name)
                } else {
                    (KIND_LABEL_FIELD, self.indices.field_name)
                };

                if let Some(name_node) = mat.captures.iter().find(|c| c.index == name_idx).map(|c| c.node) {
                    let fqn = self.get_fqn_for_definition(&name_node, source, package.as_deref());
                    let name = name_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    let range = range_from_ts(anchor_node.range());
                    let name_range = range_from_ts(name_node.range());

                    if !entities_map.contains_key(&fqn) {
                        let new_idx = entities.len();
                        let element = match kind {
                            KIND_LABEL_CLASS => JavaElement::Class(JavaClass {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_INTERFACE => JavaElement::Interface(JavaInterface {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_ENUM => JavaElement::Enum(JavaEnum {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                constants: vec![], range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_ANNOTATION => JavaElement::Annotation(JavaAnnotation {
                                id: fqn.clone(), name: name.clone(), modifiers: vec![],
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_METHOD | KIND_LABEL_CONSTRUCTOR => JavaElement::Method(JavaMethod {
                                id: fqn.clone(), name: name.clone(), return_type: TypeRef::raw("void"),
                                parameters: vec![], modifiers: vec![], is_constructor: kind == KIND_LABEL_CONSTRUCTOR,
                                range: Some(range), name_range: Some(name_range),
                            }),
                            KIND_LABEL_FIELD => JavaElement::Field(JavaField {
                                id: fqn.clone(), name: name.clone(), 
                                type_ref: anchor_node.child_by_field_name("type")
                                    .map(|n| self.parse_type_node(n, source))
                                    .unwrap_or(TypeRef::Unknown),
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
                        if let Some(parent_node) = self.find_next_enclosing_definition(anchor_node) {
                            if let Some(parent_name_node) = parent_node.child_by_field_name("name") {
                                let parent = self.get_fqn_for_definition(&parent_name_node, source, package.as_deref());
                                if parent != fqn {
                                    relations.push(JavaRelation {
                                        source_fqn: parent,
                                        target_name: fqn.clone(),
                                        rel_type: EdgeType::Contains,
                                        range: None,
                                    });
                                }
                            }
                        }
                    }
                }
                continue;
            }

            // 2. Metadata (Modifiers, Types, Params)
            if let Some(meta_cap) = mat.captures.iter().find(|c| {
                let i = c.index;
                i == self.indices.mods
                    || i == self.indices.class_super
                    || i == self.indices.class_inter
                    || i == self.indices.inter_ext
                    || i == self.indices.enum_interface
                    || i == self.indices.method_ret
                    || i == self.indices.field_type
                    || i == self.indices.param_match
            }) {
                if let Some(parent_node) = self.find_next_enclosing_definition(meta_cap.node) {
                    if let Some(parent_name_node) = parent_node.child_by_field_name("name") {
                        let enclosing_fqn = self.get_fqn_for_definition(&parent_name_node, source, package.as_deref());
                        if let Some(&idx) = entities_map.get(&enclosing_fqn) {
                            self.attach_metadata_to_model(mat, source, enclosing_fqn, &mut entities[idx].element, &mut relations);
                        }
                    }
                }
                continue;
            }

            // 3. Call/Instantiates relations
            if let Some(call_cap) = mat.captures.iter().find(|c| c.index == self.indices.call) {
                if let (Some(target_node), Some(parent_node)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.call_name).map(|c| c.node),
                    self.find_next_enclosing_definition(call_cap.node),
                ) {
                    if let Some(parent_name_node) = parent_node.child_by_field_name("name") {
                        let source_fqn = self.get_fqn_for_definition(&parent_name_node, source, package.as_deref());
                        let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                        relations.push(JavaRelation {
                            source_fqn,
                            target_name: target,
                            rel_type: EdgeType::Calls,
                            range: Some(range_from_ts(target_node.range())),
                        });
                    }
                }
            } else if let Some(inst_cap) = mat.captures.iter().find(|c| c.index == self.indices.inst) {
                if let (Some(target_node), Some(parent_node)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.inst_type).map(|c| c.node),
                    self.find_next_enclosing_definition(inst_cap.node),
                ) {
                    if let Some(parent_name_node) = parent_node.child_by_field_name("name") {
                        let source_fqn = self.get_fqn_for_definition(&parent_name_node, source, package.as_deref());
                        let target = target_node.utf8_text(source.as_bytes()).unwrap_or("").to_string();
                        relations.push(JavaRelation {
                            source_fqn,
                            target_name: target,
                            rel_type: EdgeType::Instantiates,
                            range: Some(range_from_ts(target_node.range())),
                        });
                    }
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

    fn attach_metadata_to_model(
        &self,
        mat: &QueryMatch,
        source: &str,
        fqn: String,
        element: &mut JavaElement,
        relations: &mut Vec<JavaRelation>,
    ) {
        // Modifiers & Annotations
        if let Some(mods_node) = mat.captures.iter().find(|c| c.index == self.indices.mods).map(|c| c.node) {
            let mut cursor = mods_node.walk();
            for child in mods_node.children(&mut cursor) {
                let kind = child.kind();
                if kind.contains("annotation") {
                    // It's an annotation. Try to extract the name.
                    // Structure usually: (marker_annotation name: (identifier))
                    // or (annotation name: (identifier) arguments: (...))
                    let name_node = child.child_by_field_name("name").unwrap_or(child);
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                         let name_str = name.to_string();
                         // Add DecoratedBy edge
                         relations.push(JavaRelation {
                             source_fqn: fqn.clone(),
                             target_name: name_str.clone(), // We might want to resolve this to FQN if possible
                             rel_type: EdgeType::DecoratedBy,
                             range: Some(range_from_ts(name_node.range())),
                         });
                         // Also add to modifiers list as string representation (e.g. "@Override")
                         // We need to reconstruct the full annotation text or just the name with @
                         // Existing logic added full text, let's keep it simple: just add the name prefixed with @ if not present
                         // Actually, child.utf8_text() gives the full annotation text "@Override"
                         if let Ok(full_text) = child.utf8_text(source.as_bytes()) {
                              let m_str = full_text.to_string();
                              self.add_modifier(element, m_str);
                         }
                    }
                } else if let Ok(m) = child.utf8_text(source.as_bytes()) {
                    let m_str = m.to_string();
                    self.add_modifier(element, m_str);
                }
            }
        }

        match element {
            JavaElement::Class(_) => {
                if let Some(s) = mat.captures.iter().find(|c| c.index == self.indices.class_super) {
                    let s_name = s.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    relations.push(JavaRelation {
                        source_fqn: fqn.clone(),
                        target_name: s_name,
                        rel_type: EdgeType::InheritsFrom,
                        range: None,
                    });
                }
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.class_inter) {
                    let i = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    relations.push(JavaRelation {
                        source_fqn: fqn.clone(),
                        target_name: i,
                        rel_type: EdgeType::Implements,
                        range: None,
                    });
                }
            }
            JavaElement::Interface(_) => {
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.inter_ext) {
                    let e = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    relations.push(JavaRelation {
                        source_fqn: fqn.clone(),
                        target_name: e,
                        rel_type: EdgeType::InheritsFrom,
                        range: None,
                    });
                }
            }
            JavaElement::Enum(_) => {
                for cc in mat.captures.iter().filter(|c| c.index == self.indices.enum_interface) {
                    let i = cc.node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    relations.push(JavaRelation {
                        source_fqn: fqn.clone(),
                        target_name: i,
                        rel_type: EdgeType::Implements,
                        range: None,
                    });
                }
            }
            JavaElement::Annotation(_) => {}
            JavaElement::Method(m) => {
                if let Some(ret) = mat.captures.iter().find(|c| c.index == self.indices.method_ret) {
                    m.return_type = self.parse_type_node(ret.node, source);
                    // Generate TypedAs edge for return type
                    self.generate_typed_as_edges(ret.node, source, &fqn, relations);
                }
                if let (Some(t_node), Some(n_node)) = (
                    mat.captures.iter().find(|c| c.index == self.indices.param_type).map(|c| c.node),
                    mat.captures.iter().find(|c| c.index == self.indices.param_name).map(|c| c.node),
                ) {
                    let t_ref = self.parse_type_node(t_node, source);
                    let n = n_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if !m.parameters.iter().any(|p| p.name == n && p.type_ref == t_ref) {
                        m.parameters.push(JavaParameter { type_ref: t_ref, name: n });
                    }
                    // Generate TypedAs edge for parameter type
                    self.generate_typed_as_edges(t_node, source, &fqn, relations);
                }
            }
            JavaElement::Field(f) => {
                if let Some(t) = mat.captures.iter().find(|c| c.index == self.indices.field_type) {
                    f.type_ref = self.parse_type_node(t.node, source);
                    // Generate TypedAs edge for field type
                    self.generate_typed_as_edges(t.node, source, &fqn, relations);
                }
            }
            JavaElement::Package(_) => {}
        }
    }

    fn add_modifier(&self, element: &mut JavaElement, m_str: String) {
        match element {
            JavaElement::Class(c) => if !c.modifiers.contains(&m_str) { c.modifiers.push(m_str); }
            JavaElement::Interface(i) => if !i.modifiers.contains(&m_str) { i.modifiers.push(m_str); }
            JavaElement::Enum(e) => if !e.modifiers.contains(&m_str) { e.modifiers.push(m_str); }
            JavaElement::Annotation(a) => if !a.modifiers.contains(&m_str) { a.modifiers.push(m_str); }
            JavaElement::Method(m_node) => if !m_node.modifiers.contains(&m_str) { m_node.modifiers.push(m_str); }
            JavaElement::Field(f) => if !f.modifiers.contains(&m_str) { f.modifiers.push(m_str); }
            JavaElement::Package(_) => {}
        }
    }

    /// Recursively extracts type references and generates TypedAs edges
    fn generate_typed_as_edges(
        &self,
        type_node: Node,
        source: &str,
        source_fqn: &str,
        relations: &mut Vec<JavaRelation>,
    ) {
        let kind = type_node.kind();
        
        // Base case: simple type identifier
        if kind == "type_identifier" {
            let type_name = type_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
            // Ignore primitive types for edge generation
            if !self.is_primitive(&type_name) {
                relations.push(JavaRelation {
                    source_fqn: source_fqn.to_string(),
                    target_name: type_name,
                    rel_type: EdgeType::TypedAs,
                    range: Some(range_from_ts(type_node.range())),
                });
            }
            return;
        }

        // Recursive cases
        let mut cursor = type_node.walk();
        for child in type_node.children(&mut cursor) {
            let child_kind = child.kind();
            match child_kind {
                "type_identifier" | "generic_type" | "type_arguments" | "wildcard" | "array_type" => {
                     self.generate_typed_as_edges(child, source, source_fqn, relations);
                },
                _ => {}
            }
        }
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        matches!(
            type_name,
            "byte" | "short" | "int" | "long" | "float" | "double" | "boolean" | "char" | "void"
        )
    }
}
