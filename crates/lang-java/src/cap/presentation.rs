use crate::JavaPlugin;
use lasso::Key;
use naviscope_api::models::DisplayGraphNode;
use naviscope_api::models::graph::{GraphNode, NodeKind};
use naviscope_api::models::symbol::{FqnReader, Symbol};
use naviscope_plugin::{NamingConvention, NodePresenter, PresentationCap};
use std::sync::Arc;

impl NodePresenter for JavaPlugin {
    fn render_display_node(&self, node: &GraphNode, fqns: &dyn FqnReader) -> DisplayGraphNode {
        let resolve_sid = |sid: u32| {
            lasso::Spur::try_from_usize(sid as usize)
                .map(|spur| fqns.resolve_atom(Symbol(spur)).to_string())
        };

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
                crate::model::JavaNodeMetadata::Class { modifiers_sids, .. }
                | crate::model::JavaNodeMetadata::Interface { modifiers_sids, .. }
                | crate::model::JavaNodeMetadata::Annotation { modifiers_sids } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .filter_map(|&s| resolve_sid(s))
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
                        .filter_map(|&s| resolve_sid(s))
                        .collect();
                    let params_str = parameters
                        .iter()
                        .enumerate()
                        .map(|(idx, p)| {
                            let param_type = if p.is_varargs {
                                match &p.type_ref {
                                    naviscope_api::models::TypeRef::Array { element, .. } => {
                                        format!("{}...", crate::model::fmt_type(element))
                                    }
                                    _ => format!("{}...", crate::model::fmt_type(&p.type_ref)),
                                }
                            } else {
                                crate::model::fmt_type(&p.type_ref)
                            };
                            format!(
                                "{}: {}",
                                resolve_sid(p.name_sid)
                                    .unwrap_or_else(|| format!("arg{}", idx)),
                                param_type
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
                        .filter_map(|&s| resolve_sid(s))
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
                crate::model::JavaIndexMetadata::Class { modifiers, .. }
                | crate::model::JavaIndexMetadata::Interface { modifiers, .. }
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
                            let param_type = if p.is_varargs {
                                match &p.type_ref {
                                    naviscope_api::models::TypeRef::Array { element, .. } => {
                                        format!("{}...", crate::model::fmt_type_uninterned(element))
                                    }
                                    _ => format!("{}...", crate::model::fmt_type_uninterned(&p.type_ref)),
                                }
                            } else {
                                crate::model::fmt_type_uninterned(&p.type_ref)
                            };
                            format!(
                                "{}: {}",
                                p.name,
                                param_type
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{JavaNodeMetadata, JavaParameterStorage};
    use lasso::Key;
    use naviscope_api::models::fqn::FqnNode;
    use naviscope_api::models::graph::{GraphNode, NodeKind};
    use naviscope_api::models::symbol::{FqnId, Symbol};
    use naviscope_api::models::TypeRef;
    use std::collections::HashMap;
    use std::sync::Arc;

    struct FakeFqnReader {
        nodes: HashMap<FqnId, FqnNode>,
        atoms: HashMap<u32, String>,
    }

    impl FqnReader for FakeFqnReader {
        fn resolve_node(&self, id: FqnId) -> Option<FqnNode> {
            self.nodes.get(&id).cloned()
        }

        fn resolve_atom(&self, atom: Symbol) -> &str {
            self.atoms
                .get(&(atom.0.into_usize() as u32))
                .map(String::as_str)
                .unwrap_or("<missing>")
        }
    }

    fn spur(id: usize) -> lasso::Spur {
        lasso::Spur::try_from_usize(id).expect("valid spur")
    }

    fn sym(id: usize) -> Symbol {
        Symbol(spur(id))
    }

    fn fake_fqns() -> FakeFqnReader {
        let mut nodes = HashMap::new();
        nodes.insert(
            FqnId(1),
            FqnNode {
                parent: None,
                name: sym(1),
                kind: NodeKind::Package,
            },
        );
        nodes.insert(
            FqnId(2),
            FqnNode {
                parent: Some(FqnId(1)),
                name: sym(2),
                kind: NodeKind::Class,
            },
        );
        nodes.insert(
            FqnId(3),
            FqnNode {
                parent: Some(FqnId(2)),
                name: sym(3),
                kind: NodeKind::Method,
            },
        );

        let mut atoms = HashMap::new();
        atoms.insert(1, "com".to_string());
        atoms.insert(2, "User".to_string());
        atoms.insert(3, "setNames".to_string());
        atoms.insert(4, "java".to_string());
        atoms.insert(5, "public".to_string());
        atoms.insert(6, "names".to_string());

        FakeFqnReader { nodes, atoms }
    }

    #[test]
    fn render_display_node_formats_varargs_signature() {
        let plugin = JavaPlugin::new().expect("plugin");
        let fqns = fake_fqns();
        let metadata = JavaNodeMetadata::Method {
            modifiers_sids: vec![5],
            return_type: TypeRef::Raw("void".to_string()),
            parameters: vec![JavaParameterStorage {
                name_sid: 6,
                type_ref: TypeRef::Array {
                    element: Box::new(TypeRef::Id("java.lang.String".to_string())),
                    dimensions: 1,
                },
                is_varargs: true,
            }],
            is_constructor: false,
        };

        let node = GraphNode {
            id: FqnId(3),
            name: sym(3),
            kind: NodeKind::Method,
            lang: sym(4),
            metadata: Arc::new(metadata),
            ..GraphNode::default()
        };

        let display = plugin.render_display_node(&node, &fqns);
        assert_eq!(display.signature.as_deref(), Some("setNames(names: String...) -> void"));
        assert_eq!(display.modifiers, vec!["public".to_string()]);
    }

    #[test]
    fn render_display_node_invalid_sid_falls_back_without_panic() {
        let plugin = JavaPlugin::new().expect("plugin");
        let fqns = fake_fqns();
        let metadata = JavaNodeMetadata::Method {
            modifiers_sids: vec![u32::MAX],
            return_type: TypeRef::Raw("void".to_string()),
            parameters: vec![JavaParameterStorage {
                name_sid: u32::MAX,
                type_ref: TypeRef::Id("java.lang.String".to_string()),
                is_varargs: false,
            }],
            is_constructor: false,
        };

        let node = GraphNode {
            id: FqnId(3),
            name: sym(3),
            kind: NodeKind::Method,
            lang: sym(4),
            metadata: Arc::new(metadata),
            ..GraphNode::default()
        };

        let display = plugin.render_display_node(&node, &fqns);
        assert_eq!(display.signature.as_deref(), Some("setNames(arg0: String) -> void"));
        assert!(display.modifiers.is_empty());
    }
}
