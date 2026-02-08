pub mod discoverer;
pub mod inference;
pub mod jdk;
pub mod lsp;
pub mod model;
pub mod naming;
pub mod parser;
pub mod queries;
pub mod resolver;

pub use discoverer::JdkDiscoverer;

use lasso::Key;
use naviscope_api::models::graph::{EmptyMetadata, GraphNode, NodeKind};
use naviscope_api::models::symbol::{FqnReader, Symbol};
use naviscope_api::models::{DisplayGraphNode, Language};
use naviscope_plugin::{
    AssetIndexer, AssetSourceLocator, GlobalParseResult, LangResolver, LanguagePlugin, LspService,
    NamingConvention, NodeAdapter, PluginInstance, SemanticResolver, StorageContext,
};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
}

impl NodeAdapter for JavaPlugin {
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
            // Real-time calculation from JavaIndexMetadata (Uninterned)
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

    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn StorageContext,
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
            rmp_serde::to_vec(&java_idx_meta).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn decode_metadata(
        &self,
        bytes: &[u8],
        _ctx: &dyn StorageContext,
    ) -> Arc<dyn naviscope_api::models::graph::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::JavaNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(EmptyMetadata)
        }
    }
}

impl JavaPlugin {
    pub fn new() -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Register metadata deserializer for Java
        naviscope_plugin::register_metadata_deserializer(
            "java",
            crate::model::JavaIndexMetadata::deserialize_for_cache,
        );

        let parser = Arc::new(parser::JavaParser::new()?);
        let resolver = Arc::new(resolver::JavaResolver {
            parser: (*parser).clone(),
        });
        Ok(Self { parser, resolver })
    }
}

impl PluginInstance for JavaPlugin {
    fn get_naming_convention(&self) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        Some(Arc::new(crate::naming::JavaNamingConvention::default()))
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

    fn parse_file(
        &self,
        source: &str,
        path: &Path,
    ) -> std::result::Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>> {
        self.parser.parse_file(source, Some(path))
    }

    fn resolver(&self) -> Arc<dyn SemanticResolver> {
        self.resolver.clone()
    }

    fn lang_resolver(&self) -> Arc<dyn LangResolver> {
        self.resolver.clone()
    }

    fn lsp_parser(&self) -> Arc<dyn LspService> {
        Arc::new(crate::lsp::JavaLspService::new(self.parser.clone()))
    }

    fn external_resolver(&self) -> Option<Arc<dyn naviscope_plugin::ExternalResolver>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
    }

    fn can_handle_external_asset(&self, ext: &str) -> bool {
        ext == "jar" || ext == "jmod" || ext == "class"
    }

    fn global_asset_discoverer(&self) -> Option<Box<dyn naviscope_plugin::AssetDiscoverer>> {
        Some(Box::new(crate::discoverer::JdkDiscoverer::new()))
    }

    fn asset_indexer(&self) -> Option<Arc<dyn AssetIndexer>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
    }

    fn asset_source_locator(&self) -> Option<Arc<dyn AssetSourceLocator>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
    }
}
