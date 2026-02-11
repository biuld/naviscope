use crate::core::NodePresenter;
use crate::naming::NamingConvention;
use naviscope_api::models::graph::NodeKind;
use std::sync::Arc;

pub trait PresentationCap: Send + Sync {
    fn naming_convention(&self) -> Option<Arc<dyn NamingConvention>> {
        None
    }

    fn node_presenter(&self) -> Option<Arc<dyn NodePresenter>> {
        None
    }

    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind;
}
