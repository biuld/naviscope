use naviscope_api::models::graph::{DisplayGraphNode, GraphNode};
use naviscope_api::models::symbol::FqnReader;

pub trait NodePresenter: Send + Sync {
    fn render_display_node(&self, node: &GraphNode, fqns: &dyn FqnReader) -> DisplayGraphNode;
}
