use crate::JavaPlugin;
use lasso::Key;
use naviscope_api::models::DisplayGraphNode;
use naviscope_api::models::graph::{GraphNode, NodeKind};
use naviscope_api::models::symbol::{FqnReader, Symbol};
use naviscope_plugin::{NamingConvention, NodePresenter, PresentationCap};
use std::sync::Arc;

impl NodePresenter for JavaPlugin {
    fn render_display_node(&self, node: &GraphNode, fqns: &dyn FqnReader) -> DisplayGraphNode {
        let mut display = DisplayGraphNode {
            id: crate::naming::JavaNamingConvention::default().render_fqn(node.id, fqns),
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: "java".to_string(),
            source: node.source.clone(),
            status: node.status,
            location: node.location.as_ref().map(|l| l.to_display(fqns)),
            detail: None,
            signature: None,
            modifiers: vec![],
            children: None,
        };

        let fqn = display.id.as_str();
        let parts: Vec<&str> = fqn.split('.').collect();
        if parts.len() > 1 {
            let container = parts[..parts.len() - 1].join(".");
            display.detail = Some(format!("*Defined in `{}`*", container));
        }

        if let Some(java_meta) = node
            .metadata
            .as_any()
            .downcast_ref::<crate::model::JavaNodeMetadata>()
        {
            match java_meta {
                crate::model::JavaNodeMetadata::Class { modifiers_sids }
                | crate::model::JavaNodeMetadata::Interface { modifiers_sids }
                | crate::model::JavaNodeMetadata::Annotation { modifiers_sids } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .map(|&s| {
                            fqns.resolve_atom(Symbol(
                                lasso::Spur::try_from_usize(s as usize).unwrap(),
                            ))
                            .to_string()
                        })
                        .collect();
                    let prefix = match node.kind {
                        NodeKind::Interface => "interface",
                        NodeKind::Annotation => "@interface",
                        _ => "class",
                    };
                    display.signature = Some(format!("{} {}", prefix, display.name));
                }
                crate::model::JavaNodeMetadata::Method {
                    modifiers_sids,
                    return_type,
                    parameters,
                    is_constructor,
                } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .map(|&s| {
                            fqns.resolve_atom(Symbol(
                                lasso::Spur::try_from_usize(s as usize).unwrap(),
                            ))
                            .to_string()
                        })
                        .collect();
                    let params_str = parameters
                        .iter()
                        .map(|p| {
                            format!(
                                "{}: {}",
                                fqns.resolve_atom(Symbol(
                                    lasso::Spur::try_from_usize(p.name_sid as usize).unwrap(),
                                )),
                                crate::model::fmt_type(&p.type_ref)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    if *is_constructor {
                        display.signature = Some(format!("{}({})", display.name, params_str));
                    } else {
                        display.signature = Some(format!(
                            "{}({}) -> {}",
                            display.name,
                            params_str,
                            crate::model::fmt_type(return_type)
                        ));
                    }
                }
                crate::model::JavaNodeMetadata::Field {
                    modifiers_sids,
                    type_ref,
                } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .map(|&s| {
                            fqns.resolve_atom(Symbol(
                                lasso::Spur::try_from_usize(s as usize).unwrap(),
                            ))
                            .to_string()
                        })
                        .collect();
                    display.signature = Some(format!(
                        "{}: {}",
                        display.name,
                        crate::model::fmt_type(type_ref)
                    ));
                }
                _ => {}
            }
        } else if let Some(java_idx_meta) = node
            .metadata
            .as_any()
            .downcast_ref::<crate::model::JavaIndexMetadata>()
        {
            match java_idx_meta {
                crate::model::JavaIndexMetadata::Class { modifiers }
                | crate::model::JavaIndexMetadata::Interface { modifiers }
                | crate::model::JavaIndexMetadata::Annotation { modifiers } => {
                    display.modifiers = modifiers.clone();
                    let prefix = match node.kind {
                        NodeKind::Interface => "interface",
                        NodeKind::Annotation => "@interface",
                        _ => "class",
                    };
                    display.signature = Some(format!("{} {}", prefix, display.name));
                }
                crate::model::JavaIndexMetadata::Method {
                    modifiers,
                    return_type,
                    parameters,
                    is_constructor,
                } => {
                    display.modifiers = modifiers.clone();
                    let params_str = parameters
                        .iter()
                        .map(|p| {
                            format!(
                                "{}: {}",
                                p.name,
                                crate::model::fmt_type_uninterned(&p.type_ref)
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ");
                    if *is_constructor {
                        display.signature = Some(format!("{}({})", display.name, params_str));
                    } else {
                        display.signature = Some(format!(
                            "{}({}) -> {}",
                            display.name,
                            params_str,
                            crate::model::fmt_type_uninterned(return_type)
                        ));
                    }
                }
                crate::model::JavaIndexMetadata::Field {
                    modifiers,
                    type_ref,
                } => {
                    display.modifiers = modifiers.clone();
                    display.signature = Some(format!(
                        "{}: {}",
                        display.name,
                        crate::model::fmt_type_uninterned(type_ref)
                    ));
                }
                crate::model::JavaIndexMetadata::Enum { modifiers, .. } => {
                    display.modifiers = modifiers.clone();
                    display.signature = Some(format!("enum {}", display.name));
                }
                _ => {}
            }
        }

        display
    }
}

impl PresentationCap for JavaPlugin {
    fn naming_convention(&self) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        Some(Arc::new(crate::naming::JavaNamingConvention::default()))
    }

    fn node_presenter(&self) -> Option<Arc<dyn NodePresenter>> {
        Some(Arc::new(self.clone()))
    }

    fn symbol_kind(&self, kind: &NodeKind) -> lsp_types::SymbolKind {
        use lsp_types::SymbolKind;
        match kind {
            NodeKind::Class => SymbolKind::CLASS,
            NodeKind::Interface => SymbolKind::INTERFACE,
            NodeKind::Enum => SymbolKind::ENUM,
            NodeKind::Annotation => SymbolKind::INTERFACE,
            NodeKind::Method => SymbolKind::METHOD,
            NodeKind::Constructor => SymbolKind::CONSTRUCTOR,
            NodeKind::Field => SymbolKind::FIELD,
            NodeKind::Package => SymbolKind::PACKAGE,
            _ => SymbolKind::VARIABLE,
        }
    }
}
