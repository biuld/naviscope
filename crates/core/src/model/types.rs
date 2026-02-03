// Re-export core models from API layer for internal use
pub use super::metadata::{IndexMetadata, SymbolInterner};
pub use naviscope_api::models::{
    DisplayGraphNode, DisplaySymbolLocation, EdgeType, EmptyMetadata, GraphEdge, GraphNode,
    InternedLocation, Language, NodeKind, NodeMetadata, QueryResultEdge, Range, SymbolLocation,
};

pub type NodeLocation = SymbolLocation;

pub use naviscope_plugin::{GraphOp, ResolvedUnit};

pub mod util {
    pub fn line_col_at_to_offset(content: &str, line: usize, col: usize) -> Option<usize> {
        let mut offset = 0;
        for (i, l) in content.lines().enumerate() {
            if i == line {
                if col <= l.len() {
                    return Some(offset + col);
                }
                return None;
            }
            offset += l.len() + 1; // +1 for newline
        }
        None
    }
}
