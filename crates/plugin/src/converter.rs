use crate::interner::FqnInterner;
use naviscope_api::models::graph::{DisplayGraphNode, DisplaySymbolLocation, GraphNode};
use naviscope_api::models::symbol::InternedLocation;
use std::sync::Arc;

/// Helper trait to convert public display models back to internal graph representations.
pub trait ModelConverter {
    type Output;
    fn to_internal(&self, interner: &dyn FqnInterner) -> Self::Output;
}

impl ModelConverter for DisplaySymbolLocation {
    type Output = InternedLocation;
    fn to_internal(&self, interner: &dyn FqnInterner) -> Self::Output {
        InternedLocation {
            path: interner.intern_atom(&self.path),
            range: self.range,
            selection_range: self.selection_range,
        }
    }
}

impl ModelConverter for DisplayGraphNode {
    type Output = GraphNode;
    fn to_internal(&self, interner: &dyn FqnInterner) -> Self::Output {
        let fqn_id = interner.intern_node(None, &self.id, self.kind.clone());
        GraphNode {
            id: fqn_id,
            name: interner.intern_atom(&self.name),
            kind: self.kind.clone(),
            lang: interner.intern_atom(&self.lang),
            source: self.source.clone(),
            location: self.location.as_ref().map(|l| l.to_internal(interner)),
            metadata: Arc::new(naviscope_api::models::graph::EmptyMetadata),
        }
    }
}
