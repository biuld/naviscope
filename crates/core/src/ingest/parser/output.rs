use crate::model::IndexMetadata;
use crate::model::{EdgeType, NodeKind, Range};
use naviscope_api::models::DisplaySymbolLocation;
use std::sync::Arc;

pub use naviscope_api::models::symbol::NodeId;

/// Node model during the parsing phase, before interning
/// It holds raw Strings and strongly-typed Metadata
#[derive(Debug, Clone)]
pub struct IndexNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub lang: String,
    pub location: Option<DisplaySymbolLocation>,
    pub metadata: Arc<dyn IndexMetadata>,
}

/// Relation model during the parsing phase
#[derive(Debug, Clone)]
pub struct IndexRelation {
    pub source_id: NodeId,
    pub target_id: NodeId,
    pub edge_type: EdgeType,
    pub range: Option<Range>,
}

/// Core model produced by the parser
#[derive(Debug, Clone, Default)]
pub struct ParseOutput {
    pub nodes: Vec<IndexNode>,
    pub relations: Vec<IndexRelation>,
    /// All identifiers appearing in the file (used for global search and reference indexing)
    pub identifiers: Vec<String>,
}
