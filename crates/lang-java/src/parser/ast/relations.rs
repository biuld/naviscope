use super::super::JavaParser;
use super::JavaRelation;
use naviscope_core::ingest::parser::utils::range_from_ts;
use naviscope_core::model::EdgeType;
use tree_sitter::Node;

impl JavaParser {
    pub(super) fn generate_typed_as_edges<'a>(
        &self,
        type_node: Node<'a>,
        source: &'a str,
        source_id: &naviscope_api::models::symbol::NodeId,
        relations: &mut Vec<JavaRelation>,
    ) {
        let kind = type_node.kind();
        if kind == "type_identifier" {
            let type_name = type_node
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_string();
            if !self.is_primitive(&type_name) {
                relations.push(JavaRelation {
                    source_id: source_id.clone(),
                    target_id: naviscope_api::models::symbol::NodeId::Flat(type_name),
                    rel_type: EdgeType::TypedAs,
                    range: Some(range_from_ts(type_node.range())),
                });
            }
            return;
        }

        let mut cursor = type_node.walk();
        for child in type_node.children(&mut cursor) {
            if matches!(
                child.kind(),
                "type_identifier" | "generic_type" | "type_arguments" | "wildcard" | "array_type"
            ) {
                self.generate_typed_as_edges(child, source, source_id, relations);
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
