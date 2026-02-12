use super::super::JavaParser;
use super::{JavaEntity, JavaRelation};
use crate::model::{JavaIndexMetadata, JavaParameter};
use naviscope_api::models::graph::EdgeType;
use naviscope_plugin::utils::range_from_ts;
use std::collections::HashMap;
use tree_sitter::QueryCapture;

impl JavaParser {
    pub(super) fn enrich_metadata<'a>(
        &self,
        all_matches: &[Vec<QueryCapture<'a>>],
        source: &'a str,
        package: &Option<String>,
        entities: &mut [JavaEntity<'a>],
        relations: &mut Vec<JavaRelation>,
        entities_map: &HashMap<naviscope_api::models::symbol::NodeId, usize>,
    ) {
        for captures in all_matches {
            if let Some(meta_cap) = captures.iter().find(|c| {
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
                        let pk = Self::tree_sitter_kind_to_node_kind(parent_node.kind());
                        if let Some(pk) = pk {
                            let enclosing_id = self.get_node_id_for_definition(
                                &parent_name_node,
                                source,
                                package.as_deref(),
                                pk,
                            );
                            if let Some(&idx) = entities_map.get(&enclosing_id) {
                                let fqn_id = entities[idx].fqn.clone();
                                self.attach_metadata_to_model(
                                    captures,
                                    source,
                                    fqn_id,
                                    &mut entities[idx].element,
                                    relations,
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    fn attach_metadata_to_model<'a>(
        &self,
        captures: &[QueryCapture<'a>],
        source: &'a str,
        fqn_id: naviscope_api::models::symbol::NodeId,
        element: &mut JavaIndexMetadata,
        relations: &mut Vec<JavaRelation>,
    ) {
        // Modifiers & Annotations
        if let Some(mods_node) = captures
            .iter()
            .find(|c| c.index == self.indices.mods)
            .map(|c| c.node)
        {
            let mut cursor = mods_node.walk();
            for child in mods_node.children(&mut cursor) {
                let kind = child.kind();
                if kind.contains("annotation") {
                    let name_node = child.child_by_field_name("name").unwrap_or(child);
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                        let mut name_str = name.to_string();
                        if name_str.starts_with('@') {
                            name_str = name_str[1..].to_string();
                        }
                        relations.push(JavaRelation {
                            source_id: fqn_id.clone(),
                            target_id: naviscope_api::models::symbol::NodeId::Flat(name_str),
                            rel_type: EdgeType::DecoratedBy,
                            range: Some(range_from_ts(name_node.range())),
                        });
                        if let Ok(full_text) = child.utf8_text(source.as_bytes()) {
                            self.add_modifier(element, full_text.to_string());
                        }
                    }
                } else if let Ok(m) = child.utf8_text(source.as_bytes()) {
                    self.add_modifier(element, m.to_string());
                }
            }
        }

        match element {
            JavaIndexMetadata::Class { .. } => {
                if let Some(s) = captures
                    .iter()
                    .find(|c| c.index == self.indices.class_super)
                {
                    let mut s_name = s
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    let mut cursor = s.node.walk();
                    for child in s.node.children(&mut cursor) {
                        if matches!(
                            child.kind(),
                            "type_identifier" | "scoped_type_identifier" | "generic_type"
                        ) {
                            s_name = child
                                .utf8_text(source.as_bytes())
                                .unwrap_or_default()
                                .to_string();
                            break;
                        }
                    }
                    relations.push(JavaRelation {
                        source_id: fqn_id.clone(),
                        target_id: naviscope_api::models::symbol::NodeId::Flat(s_name),
                        rel_type: EdgeType::InheritsFrom,
                        range: Some(range_from_ts(s.node.range())),
                    });
                }
                for cc in captures
                    .iter()
                    .filter(|c| c.index == self.indices.class_inter)
                {
                    let i = cc
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    relations.push(JavaRelation {
                        source_id: fqn_id.clone(),
                        target_id: naviscope_api::models::symbol::NodeId::Flat(i),
                        rel_type: EdgeType::Implements,
                        range: Some(range_from_ts(cc.node.range())),
                    });
                }
            }
            JavaIndexMetadata::Interface { .. } => {
                for cc in captures
                    .iter()
                    .filter(|c| c.index == self.indices.inter_ext)
                {
                    let e = cc
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    relations.push(JavaRelation {
                        source_id: fqn_id.clone(),
                        target_id: naviscope_api::models::symbol::NodeId::Flat(e),
                        rel_type: EdgeType::InheritsFrom,
                        range: Some(range_from_ts(cc.node.range())),
                    });
                }
            }
            JavaIndexMetadata::Enum {
                modifiers: _,
                constants: _,
            } => {
                for cc in captures
                    .iter()
                    .filter(|c| c.index == self.indices.enum_interface)
                {
                    let i = cc
                        .node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    relations.push(JavaRelation {
                        source_id: fqn_id.clone(),
                        target_id: naviscope_api::models::symbol::NodeId::Flat(i),
                        rel_type: EdgeType::Implements,
                        range: Some(range_from_ts(cc.node.range())),
                    });
                }
            }
            JavaIndexMetadata::Method {
                modifiers: _,
                return_type,
                parameters,
                is_constructor: _,
            } => {
                if let Some(ret) = captures.iter().find(|c| c.index == self.indices.method_ret) {
                    *return_type = self.parse_type_node(ret.node, source);
                    self.generate_typed_as_edges(ret.node, source, &fqn_id, relations);
                }
                if let (Some(t_node), Some(n_node)) = (
                    captures
                        .iter()
                        .find(|c| c.index == self.indices.param_type)
                        .map(|c| c.node),
                    captures
                        .iter()
                        .find(|c| c.index == self.indices.param_name)
                        .map(|c| c.node),
                ) {
                    let t_ref = self.parse_type_node(t_node, source);
                    let n = n_node
                        .utf8_text(source.as_bytes())
                        .unwrap_or_default()
                        .to_string();
                    let is_varargs = captures
                        .iter()
                        .find(|c| c.index == self.indices.param_match)
                        .map(|c| c.node.kind() == "spread_parameter")
                        .unwrap_or(false);
                    if !parameters
                        .iter()
                        .any(|p| p.name == n && p.type_ref == t_ref && p.is_varargs == is_varargs)
                    {
                        parameters.push(JavaParameter {
                            type_ref: t_ref,
                            name: n,
                            is_varargs,
                        });
                    }
                    self.generate_typed_as_edges(t_node, source, &fqn_id, relations);
                }
            }
            JavaIndexMetadata::Field {
                modifiers: _,
                type_ref,
            } => {
                if let Some(t) = captures.iter().find(|c| c.index == self.indices.field_type) {
                    *type_ref = self.parse_type_node(t.node, source);
                    self.generate_typed_as_edges(t.node, source, &fqn_id, relations);
                }
            }
            _ => {}
        }
    }

    fn add_modifier(&self, element: &mut JavaIndexMetadata, m_str: String) {
        match element {
            JavaIndexMetadata::Class { modifiers, .. } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Interface { modifiers, .. } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Enum {
                modifiers,
                constants: _,
            } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Annotation { modifiers } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Method {
                modifiers,
                return_type: _,
                parameters: _,
                is_constructor: _,
            } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Field {
                modifiers,
                type_ref: _,
            } => {
                if !modifiers.contains(&m_str) {
                    modifiers.push(m_str);
                }
            }
            JavaIndexMetadata::Package => {}
        }
    }
}
