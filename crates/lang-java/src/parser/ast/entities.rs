use super::super::JavaParser;
use super::super::constants::*;
use super::{JavaEntity, JavaRelation};
use crate::model::*;
use naviscope_api::models::TypeRef;
use naviscope_api::models::graph::EdgeType;
use naviscope_api::models::symbol::Range;
use naviscope_plugin::utils::range_from_ts;
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
        entities_map: &mut HashMap<naviscope_api::models::symbol::NodeId, usize>,
    ) {
        use naviscope_api::models::graph::NodeKind;

        // Helper to map string label to NodeKind
        let map_kind_label = |label: &str| -> NodeKind {
            match label {
                KIND_LABEL_CLASS => NodeKind::Class,
                KIND_LABEL_INTERFACE => NodeKind::Interface,
                KIND_LABEL_ENUM => NodeKind::Enum,
                KIND_LABEL_ANNOTATION => NodeKind::Annotation,
                KIND_LABEL_METHOD => NodeKind::Method,
                KIND_LABEL_CONSTRUCTOR => NodeKind::Constructor,
                KIND_LABEL_FIELD => NodeKind::Field,
                _ => NodeKind::Custom(label.to_string()),
            }
        };

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
                let (kind_label, name_idx) = if anchor.index == self.indices.class_def {
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
                    // Generate structured ID
                    let node_kind = map_kind_label(kind_label);
                    let fqn_id = self.get_node_id_for_definition(
                        &name_node,
                        source,
                        package.as_deref(),
                        node_kind.clone(),
                    );

                    let name = name_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    let range = range_from_ts(anchor_node.range());
                    let name_range = range_from_ts(name_node.range());

                    if !entities_map.contains_key(&fqn_id) {
                        let new_idx = entities.len();
                        let element = self.create_java_element(
                            kind_label, &fqn_id, &name, range, name_range, captures, source,
                            relations,
                        );
                        entities.push(JavaEntity {
                            element,
                            node: anchor_node,
                            fqn: fqn_id.clone(),
                            name: name.clone(),
                        });
                        entities_map.insert(fqn_id.clone(), new_idx);

                        // Structural relation (Contains)
                        if let Some(parent_node) = self.find_next_enclosing_definition(anchor_node)
                        {
                            if let Some(parent_name_node) = parent_node.child_by_field_name("name")
                            {
                                // Kind of parent is needed
                                let pk = Self::tree_sitter_kind_to_node_kind(parent_node.kind());

                                if let Some(pk) = pk {
                                    let parent_id = self.get_node_id_for_definition(
                                        &parent_name_node,
                                        source,
                                        package.as_deref(),
                                        pk,
                                    );

                                    if parent_id != fqn_id {
                                        relations.push(JavaRelation {
                                            source_id: parent_id,
                                            target_id: fqn_id.clone(),
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
    }

    fn create_java_element<'a>(
        &self,
        kind: &str,
        fqn_id: &naviscope_api::models::symbol::NodeId,
        _name: &str,
        _range: Range,
        _name_range: Range,
        captures: &[QueryCapture<'a>],
        source: &'a str,
        relations: &mut Vec<JavaRelation>,
    ) -> JavaIndexMetadata {
        match kind {
            KIND_LABEL_CLASS => JavaIndexMetadata::Class {
                modifiers: vec![],
                type_parameters: self.extract_type_parameters(captures, source),
            },
            KIND_LABEL_INTERFACE => JavaIndexMetadata::Interface {
                modifiers: vec![],
                type_parameters: self.extract_type_parameters(captures, source),
            },
            KIND_LABEL_ENUM => JavaIndexMetadata::Enum {
                modifiers: vec![],
                constants: vec![],
            },
            KIND_LABEL_ANNOTATION => JavaIndexMetadata::Annotation { modifiers: vec![] },
            KIND_LABEL_METHOD | KIND_LABEL_CONSTRUCTOR => {
                let def_idx = if kind == KIND_LABEL_METHOD {
                    self.indices.method_def
                } else {
                    self.indices.constr_def
                };
                let anchor_node = captures
                    .iter()
                    .find(|c| c.index == def_idx)
                    .map(|c| c.node)
                    .expect("Method definition node must exist");

                let mut return_type = TypeRef::raw("void");
                if let Some(ret_node) = captures
                    .iter()
                    .find(|c| c.index == self.indices.method_ret)
                    .map(|c| c.node)
                {
                    return_type = self.parse_type_node(ret_node, source);
                    self.generate_typed_as_edges(ret_node, source, fqn_id, relations);
                }
                JavaIndexMetadata::Method {
                    return_type,
                    parameters: self.extract_method_parameters(anchor_node, source),
                    modifiers: vec![],
                    is_constructor: kind == KIND_LABEL_CONSTRUCTOR,
                }
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
                    self.generate_typed_as_edges(t, source, fqn_id, relations);
                    self.parse_type_node(t, source)
                } else {
                    TypeRef::Unknown
                };

                JavaIndexMetadata::Field {
                    type_ref,
                    modifiers: vec![],
                }
            }
            _ => unreachable!(),
        }
    }

    fn extract_type_parameters<'a>(
        &self,
        captures: &[QueryCapture<'a>],
        source: &'a str,
    ) -> Vec<String> {
        let declaration_node = captures.iter().find_map(|c| {
            if c.index == self.indices.class_def || c.index == self.indices.inter_def {
                Some(c.node)
            } else {
                None
            }
        });

        let Some(declaration_node) = declaration_node else {
            return Vec::new();
        };

        let type_params_node = declaration_node
            .child_by_field_name("type_parameters")
            .or_else(|| {
                let mut cursor = declaration_node.walk();
                declaration_node
                    .children(&mut cursor)
                    .find(|n| n.kind() == "type_parameters")
            });

        let Some(type_params_node) = type_params_node else {
            return Vec::new();
        };

        let mut result = Vec::new();
        let mut cursor = type_params_node.walk();
        for child in type_params_node.children(&mut cursor) {
            if child.kind() != "type_parameter" {
                continue;
            }

            if let Some(name_node) = child.child_by_field_name("name") {
                if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                    result.push(name.to_string());
                }
                continue;
            }

            let mut type_param_cursor = child.walk();
            for gc in child.children(&mut type_param_cursor) {
                if gc.kind() == "type_identifier" {
                    if let Ok(name) = gc.utf8_text(source.as_bytes()) {
                        result.push(name.to_string());
                    }
                    break;
                }
            }
        }

        result
    }
}
