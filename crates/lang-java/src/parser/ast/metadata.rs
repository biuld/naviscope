use super::super::JavaParser;
use super::{JavaEntity, JavaRelation};
use crate::model::{JavaElement, JavaParameter};
use naviscope_core::model::graph::EdgeType;
use naviscope_core::parser::utils::range_from_ts;
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
        entities_map: &HashMap<String, usize>,
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
                        let enclosing_fqn = self.get_fqn_for_definition(
                            &parent_name_node,
                            source,
                            package.as_deref(),
                        );
                        if let Some(&idx) = entities_map.get(&enclosing_fqn) {
                            self.attach_metadata_to_model(
                                captures,
                                source,
                                enclosing_fqn,
                                &mut entities[idx].element,
                                relations,
                            );
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
        fqn: String,
        element: &mut JavaElement,
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
                            source_fqn: fqn.clone(),
                            target_name: name_str,
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
            JavaElement::Class(_) => {
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
                        source_fqn: fqn.clone(),
                        target_name: s_name,
                        rel_type: EdgeType::InheritsFrom,
                        range: None,
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
                        source_fqn: fqn.clone(),
                        target_name: i,
                        rel_type: EdgeType::Implements,
                        range: None,
                    });
                }
            }
            JavaElement::Interface(_) => {
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
                        source_fqn: fqn.clone(),
                        target_name: e,
                        rel_type: EdgeType::InheritsFrom,
                        range: None,
                    });
                }
            }
            JavaElement::Enum(_) => {
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
                        source_fqn: fqn.clone(),
                        target_name: i,
                        rel_type: EdgeType::Implements,
                        range: None,
                    });
                }
            }
            JavaElement::Method(m) => {
                if let Some(ret) = captures.iter().find(|c| c.index == self.indices.method_ret) {
                    m.return_type = self.parse_type_node(ret.node, source);
                    self.generate_typed_as_edges(ret.node, source, &fqn, relations);
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
                    if !m
                        .parameters
                        .iter()
                        .any(|p| p.name == n && p.type_ref == t_ref)
                    {
                        m.parameters.push(JavaParameter {
                            type_ref: t_ref,
                            name: n,
                        });
                    }
                    self.generate_typed_as_edges(t_node, source, &fqn, relations);
                }
            }
            JavaElement::Field(f) => {
                if let Some(t) = captures.iter().find(|c| c.index == self.indices.field_type) {
                    f.type_ref = self.parse_type_node(t.node, source);
                    self.generate_typed_as_edges(t.node, source, &fqn, relations);
                }
            }
            _ => {}
        }
    }

    fn add_modifier(&self, element: &mut JavaElement, m_str: String) {
        match element {
            JavaElement::Class(c) => {
                if !c.modifiers.contains(&m_str) {
                    c.modifiers.push(m_str);
                }
            }
            JavaElement::Interface(i) => {
                if !i.modifiers.contains(&m_str) {
                    i.modifiers.push(m_str);
                }
            }
            JavaElement::Enum(e) => {
                if !e.modifiers.contains(&m_str) {
                    e.modifiers.push(m_str);
                }
            }
            JavaElement::Annotation(a) => {
                if !a.modifiers.contains(&m_str) {
                    a.modifiers.push(m_str);
                }
            }
            JavaElement::Method(m) => {
                if !m.modifiers.contains(&m_str) {
                    m.modifiers.push(m_str);
                }
            }
            JavaElement::Field(f) => {
                if !f.modifiers.contains(&m_str) {
                    f.modifiers.push(m_str);
                }
            }
            JavaElement::Package(_) => {}
        }
    }
}
