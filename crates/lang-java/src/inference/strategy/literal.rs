use super::InferStrategy;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

pub struct LiteralInfer;

impl InferStrategy for LiteralInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        let kind = node.kind();

        match kind {
            "decimal_integer_literal"
            | "hex_integer_literal"
            | "octal_integer_literal"
            | "binary_integer_literal" => {
                if let Ok(text) = node.utf8_text(ctx.source.as_bytes()) {
                    if text.ends_with('L') || text.ends_with('l') {
                        return Some(TypeRef::Raw("long".to_string()));
                    }
                }
                Some(TypeRef::Raw("int".to_string()))
            }
            "decimal_floating_point_literal" | "hex_floating_point_literal" => {
                if let Ok(text) = node.utf8_text(ctx.source.as_bytes()) {
                    if text.ends_with('f') || text.ends_with('F') {
                        return Some(TypeRef::Raw("float".to_string()));
                    }
                }
                Some(TypeRef::Raw("double".to_string()))
            }
            "true" | "false" => Some(TypeRef::Raw("boolean".to_string())),
            "character_literal" => Some(TypeRef::Raw("char".to_string())),
            "string_literal" => Some(TypeRef::Id("java.lang.String".to_string())),
            "null_literal" => Some(TypeRef::Unknown), // Null is compatible with any ref type, handled by assignment check usually
            _ => None,
        }
    }
}
