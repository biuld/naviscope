use super::super::JavaParser;
use super::super::constants::*;
use super::{JavaEntity, JavaRelation};
use crate::model::graph::{EdgeType, Range};
use crate::model::lang::java::{
    JavaAnnotation, JavaClass, JavaElement, JavaEnum, JavaField, JavaInterface, JavaMethod,
};
use crate::model::signature::TypeRef;
use crate::parser::utils::range_from_ts;
use std::collections::HashMap;
use tree_sitter::QueryCapture;

impl JavaParser {
    pub(crate) fn identify_entities<'a>(
        &self,
        all_matches: &[Vec<QueryCapture<'a>>],
        source: &'a str,
        package: &Option<String>,
        entities: &mut Vec<JavaEntity<'a>>,
        relations: &mut Vec<JavaRelation>,
        entities_map: &mut HashMap<String, usize>,
    ) {
        for captures in all_matches {
            let definition_anchor = captures.iter().find(|c| {
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

                if let Some(name_node) = captures
                    .iter()
                    .find(|c| c.index == name_idx)
                    .map(|c| c.node)
                {
                    let fqn = self.get_fqn_for_definition(&name_node, source, package.as_deref());
                    let name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    let range = range_from_ts(anchor_node.range());
                    let name_range = range_from_ts(name_node.range());

                    if !entities_map.contains_key(&fqn) {
                        let new_idx = entities.len();
                        let element = self.create_java_element(
                            kind, &fqn, &name, range, name_range, captures, source, relations,
                        );
                        entities.push(JavaEntity {
                            element,
                            node: anchor_node,
                        });
                        entities_map.insert(fqn.clone(), new_idx);

                        // Structural relation (Contains)
                        if let Some(parent_node) = self.find_next_enclosing_definition(anchor_node)
                        {
                            if let Some(parent_name_node) = parent_node.child_by_field_name("name")
                            {
                                let parent = self.get_fqn_for_definition(
                                    &parent_name_node,
                                    source,
                                    package.as_deref(),
                                );
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
            }
        }
    }

    fn create_java_element<'a>(
        &self,
        kind: &str,
        fqn: &str,
        name: &str,
        range: Range,
        name_range: Range,
        captures: &[QueryCapture<'a>],
        source: &'a str,
        relations: &mut Vec<JavaRelation>,
    ) -> JavaElement {
        match kind {
            KIND_LABEL_CLASS => JavaElement::Class(JavaClass {
                id: fqn.to_string(),
                name: name.to_string(),
                modifiers: vec![],
                range: Some(range),
                name_range: Some(name_range),
            }),
            KIND_LABEL_INTERFACE => JavaElement::Interface(JavaInterface {
                id: fqn.to_string(),
                name: name.to_string(),
                modifiers: vec![],
                range: Some(range),
                name_range: Some(name_range),
            }),
            KIND_LABEL_ENUM => JavaElement::Enum(JavaEnum {
                id: fqn.to_string(),
                name: name.to_string(),
                modifiers: vec![],
                constants: vec![],
                range: Some(range),
                name_range: Some(name_range),
            }),
            KIND_LABEL_ANNOTATION => JavaElement::Annotation(JavaAnnotation {
                id: fqn.to_string(),
                name: name.to_string(),
                modifiers: vec![],
                range: Some(range),
                name_range: Some(name_range),
            }),
            KIND_LABEL_METHOD | KIND_LABEL_CONSTRUCTOR => {
                let mut return_type = TypeRef::raw("void");
                if let Some(ret_node) = captures
                    .iter()
                    .find(|c| c.index == self.indices.method_ret)
                    .map(|c| c.node)
                {
                    return_type = self.parse_type_node(ret_node, source);
                    self.generate_typed_as_edges(ret_node, source, fqn, relations);
                }
                JavaElement::Method(JavaMethod {
                    id: fqn.to_string(),
                    name: name.to_string(),
                    return_type,
                    parameters: vec![],
                    modifiers: vec![],
                    is_constructor: kind == KIND_LABEL_CONSTRUCTOR,
                    range: Some(range),
                    name_range: Some(name_range),
                })
            }
            KIND_LABEL_FIELD => {
                let anchor_node = captures
                    .iter()
                    .find(|c| c.index == self.indices.field_def)
                    .unwrap()
                    .node;
                let type_node = captures
                    .iter()
                    .find(|c| c.index == self.indices.field_type)
                    .map(|c| c.node)
                    .or_else(|| anchor_node.child_by_field_name("type"))
                    .or_else(|| {
                        anchor_node
                            .parent()
                            .and_then(|p| p.child_by_field_name("type"))
                    });

                let type_ref = if let Some(t) = type_node {
                    self.generate_typed_as_edges(t, source, fqn, relations);
                    self.parse_type_node(t, source)
                } else {
                    TypeRef::Unknown
                };

                JavaElement::Field(JavaField {
                    id: fqn.to_string(),
                    name: name.to_string(),
                    type_ref,
                    modifiers: vec![],
                    range: Some(range),
                    name_range: Some(name_range),
                })
            }
            _ => unreachable!(),
        }
    }
}
