pub mod model;
pub mod naming;
pub mod parser;
pub mod queries;
pub mod resolver;

use lasso::Key;
use naviscope_api::models::DisplayGraphNode;
use naviscope_api::models::symbol::{FqnReader, Symbol};
use naviscope_core::error::Result;
use naviscope_core::ingest::parser::{GlobalParseResult, LspParser};
use naviscope_core::ingest::resolver::SemanticResolver;
use naviscope_core::model::source::Language;
use naviscope_core::plugin::{
    LanguagePlugin, NamingConvention, NodeAdapter, PluginInstance,
};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
}

impl NodeAdapter for JavaPlugin {
    fn render_display_node(
        &self,
        node: &naviscope_core::model::GraphNode,
        fqns: &dyn FqnReader,
    ) -> DisplayGraphNode {
        let mut display = DisplayGraphNode {
            id: crate::naming::JavaNamingConvention.render_fqn(node.id, fqns),
            name: fqns.resolve_atom(node.name).to_string(),
            kind: node.kind.clone(),
            lang: "java".to_string(),
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

        // Real-time calculation from JavaNodeMetadata
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
                        naviscope_core::model::NodeKind::Interface => "interface",
                        naviscope_core::model::NodeKind::Annotation => "@interface",
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
                crate::model::JavaNodeMetadata::Enum { modifiers_sids, .. } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .map(|&s| {
                            fqns.resolve_atom(Symbol(
                                lasso::Spur::try_from_usize(s as usize).unwrap(),
                            ))
                            .to_string()
                        })
                        .collect();
                    display.signature = Some(format!("enum {}", display.name));
                }
                _ => {}
            }
        } else if let Some(java_idx_meta) = node
            .metadata
            .as_any()
            .downcast_ref::<crate::model::JavaIndexMetadata>()
        {
            // Real-time calculation from JavaIndexMetadata (Uninterned)
            match java_idx_meta {
                crate::model::JavaIndexMetadata::Class { modifiers }
                | crate::model::JavaIndexMetadata::Interface { modifiers }
                | crate::model::JavaIndexMetadata::Annotation { modifiers } => {
                    display.modifiers = modifiers.clone();
                    let prefix = match node.kind {
                        naviscope_core::model::NodeKind::Interface => "interface",
                        naviscope_core::model::NodeKind::Annotation => "@interface",
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

    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        ctx: &mut dyn naviscope_api::models::graph::StorageContext,
    ) -> Vec<u8> {
        if let Some(java_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::JavaNodeMetadata>()
        {
            rmp_serde::to_vec(&java_meta).unwrap_or_default()
        } else if let Some(java_idx_meta) = metadata
            .as_any()
            .downcast_ref::<crate::model::JavaIndexMetadata>()
        {
            // Convert uninterned JavaIndexMetadata to optimized storage
            if let Some(storage_ctx) = ctx
                .as_any_mut()
                .downcast_mut::<naviscope_core::model::storage::model::GenericStorageContext>()
            {
                let storage_metadata = java_idx_meta.to_storage(storage_ctx);
                rmp_serde::to_vec(&storage_metadata).unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        }
    }

    fn decode_metadata(
        &self,
        bytes: &[u8],
        _ctx: &dyn naviscope_api::models::graph::StorageContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::JavaNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(naviscope_core::model::EmptyMetadata)
        }
    }
}

impl JavaPlugin {
    pub fn new() -> Result<Self> {
        let parser = Arc::new(parser::JavaParser::new()?);
        let resolver = Arc::new(resolver::JavaResolver {
            parser: (*parser).clone(),
        });
        Ok(Self { parser, resolver })
    }
}

impl PluginInstance for JavaPlugin {
    fn get_naming_convention(&self) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        Some(Arc::new(crate::naming::JavaNamingConvention))
    }

    fn get_node_adapter(&self) -> Option<Arc<dyn NodeAdapter>> {
        Some(Arc::new(self.clone()))
    }
}

impl LanguagePlugin for JavaPlugin {
    fn name(&self) -> Language {
        Language::JAVA
    }

    fn supported_extensions(&self) -> &[&str] {
        &["java"]
    }

    fn parse_file(&self, source: &str, path: &Path) -> Result<GlobalParseResult> {
        use naviscope_core::ingest::parser::IndexParser;
        self.parser.parse_file(source, Some(path))
    }

    fn resolver(&self) -> Arc<dyn SemanticResolver> {
        self.resolver.clone()
    }

    fn lang_resolver(&self) -> Arc<dyn naviscope_core::ingest::resolver::LangResolver> {
        self.resolver.clone()
    }

    fn lsp_parser(&self) -> Arc<dyn LspParser> {
        // Return self because JavaPlugin implements LspParser
        // and provides the "Complete" (hydrated) view.
        Arc::new(self.clone())
    }
}

impl LspParser for JavaPlugin {
    fn parse(
        &self,
        source: &str,
        old_tree: Option<&tree_sitter::Tree>,
    ) -> Option<tree_sitter::Tree> {
        self.parser.parse(source, old_tree)
    }

    fn extract_symbols(&self, tree: &tree_sitter::Tree, source: &str) -> Vec<DisplayGraphNode> {
        // Symbols are already fully rendered by the parser
        self.parser.extract_symbols(tree, source)
    }

    fn symbol_kind(&self, kind: &naviscope_core::model::NodeKind) -> lsp_types::SymbolKind {
        self.parser.symbol_kind(kind)
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &tree_sitter::Tree,
        target: &naviscope_core::ingest::parser::SymbolResolution,
    ) -> Vec<naviscope_core::model::Range> {
        self.parser.find_occurrences(source, tree, target)
    }
}
