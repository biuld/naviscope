use crate::model::graph::EdgeType;
use crate::parser::utils::range_from_ts;
use tree_sitter::{Node, QueryCapture};
use super::super::JavaParser;
use super::JavaRelation;

impl JavaParser {
    pub(super) fn resolve_relations<'a>(
        &self,
        all_matches: &[Vec<QueryCapture<'a>>],
        source: &'a str,
        package: &Option<String>,
        relations: &mut Vec<JavaRelation>,
    ) {
        for captures in all_matches {
            if let Some(call_cap) = captures.iter().find(|c| c.index == self.indices.call) {
                if let Some(target_node) = captures.iter().find(|c| c.index == self.indices.call_name).map(|c| c.node) {
                    let source_fqn = self.get_stable_enclosing_fqn(call_cap.node, source, package);
                    let mut target = target_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if let Some(obj) = call_cap.node.child_by_field_name("object") {
                        let obj_text = obj.utf8_text(source.as_bytes()).unwrap_or_default();
                        target = format!("{}.{}", obj_text, target);
                    }
                    relations.push(JavaRelation {
                        source_fqn,
                        target_name: target,
                        rel_type: EdgeType::Calls,
                        range: Some(range_from_ts(target_node.range())),
                    });
                }
            } else if let Some(inst_cap) = captures.iter().find(|c| c.index == self.indices.inst) {
                if let Some(target_node) = captures.iter().find(|c| c.index == self.indices.inst_type).map(|c| c.node) {
                    let source_fqn = self.get_stable_enclosing_fqn(inst_cap.node, source, package);
                    let target = target_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    relations.push(JavaRelation {
                        source_fqn,
                        target_name: target,
                        rel_type: EdgeType::Instantiates,
                        range: Some(range_from_ts(target_node.range())),
                    });
                }
            } else if let Some(fa_cap) = captures.iter().find(|c| c.index == self.indices.field_access_meta) {
                if let Some(target_node) = captures.iter().find(|c| c.index == self.indices.field_name_node).map(|c| c.node) {
                    let source_fqn = self.get_stable_enclosing_fqn(fa_cap.node, source, package);
                    let mut target = target_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
                    if let Some(obj) = fa_cap.node.child_by_field_name("object") {
                        let obj_text = obj.utf8_text(source.as_bytes()).unwrap_or_default();
                        target = format!("{}.{}", obj_text, target);
                    }
                    relations.push(JavaRelation {
                        source_fqn,
                        target_name: target,
                        rel_type: EdgeType::Calls,
                        range: Some(range_from_ts(target_node.range())),
                    });
                }
            }
        }
    }

    fn get_stable_enclosing_fqn<'a>(&self, node: Node<'a>, source: &'a str, package: &Option<String>) -> String {
        let mut curr = node;
        while let Some(parent) = self.find_next_enclosing_definition(curr) {
            if parent.kind() == "variable_declarator" {
                if let Some(gp) = parent.parent() {
                    if gp.kind() == "field_declaration" {
                        if let Some(name_node) = parent.child_by_field_name("name") {
                            return self.get_fqn_for_definition(&name_node, source, package.as_deref());
                        }
                    }
                }
                curr = parent;
                continue;
            }
            if let Some(name_node) = parent.child_by_field_name("name") {
                return self.get_fqn_for_definition(&name_node, source, package.as_deref());
            }
            curr = parent;
        }
        package.clone().unwrap_or_default()
    }

    pub(super) fn generate_typed_as_edges<'a>(
        &self,
        type_node: Node<'a>,
        source: &'a str,
        source_fqn: &str,
        relations: &mut Vec<JavaRelation>,
    ) {
        let kind = type_node.kind();
        if kind == "type_identifier" {
            let type_name = type_node.utf8_text(source.as_bytes()).unwrap_or_default().to_string();
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

        let mut cursor = type_node.walk();
        for child in type_node.children(&mut cursor) {
            if matches!(child.kind(), "type_identifier" | "generic_type" | "type_arguments" | "wildcard" | "array_type") {
                self.generate_typed_as_edges(child, source, source_fqn, relations);
            }
        }
    }

    fn is_primitive(&self, type_name: &str) -> bool {
        matches!(type_name, "byte" | "short" | "int" | "long" | "float" | "double" | "boolean" | "char" | "void")
    }
}
