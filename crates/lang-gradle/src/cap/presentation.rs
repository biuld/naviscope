use crate::GradlePlugin;
use naviscope_api::models::graph::{DisplayGraphNode, GraphNode, NodeKind};
use naviscope_api::models::symbol::FqnReader;
use naviscope_plugin::{NamingConvention, NodePresenter, PresentationCap, StandardNamingConvention};
use std::sync::Arc;

impl NodePresenter for GradlePlugin {
    fn render_display_node(&self, node: &GraphNode, fqns: &dyn FqnReader) -> DisplayGraphNode {
        let display_id = StandardNamingConvention.render_fqn(node.id, fqns);
        let mut display = DisplayGraphNode {
            id: display_id,
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: "gradle".to_string(),
            source: node.source.clone(),
            status: node.status,
            location: node.location.as_ref().map(|l| l.to_display(fqns)),
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        };

        if let Some(gradle_meta) = node
            .metadata
            .as_any()
            .downcast_ref::<crate::model::GradleNodeMetadata>()
        {
            display.detail = gradle_meta.detail_view(fqns);
        }

        display
    }
}

impl PresentationCap for GradlePlugin {
    fn node_presenter(&self) -> Option<Arc<dyn NodePresenter>> {
        Some(Arc::new(Self::new()))
    }

    fn symbol_kind(&self, _kind: &NodeKind) -> lsp_types::SymbolKind {
        lsp_types::SymbolKind::MODULE
    }
}
