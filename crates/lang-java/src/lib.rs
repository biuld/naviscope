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
    AssetCap, AssetDiscoverer, AssetIndexer, AssetSourceLocator, CodecContext,
    FileMatcherCap, LanguageCaps, LanguageParseCap, MetadataCodecCap, NodeMetadataCodec,
    NodePresenter, PresentationCap, ProjectContext, ReferenceCheckService, SemanticCap,
    SourceIndexCap, SymbolQueryService, SymbolResolveService, LspSyntaxService, NamingConvention,
    ParsedFile, ResolvedUnit,
};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
    type_system: Arc<lsp::type_system::JavaTypeSystem>,
}

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

}

impl NodeMetadataCodec for JavaPlugin {
    fn encode_metadata(
        &self,
        metadata: &dyn naviscope_api::models::graph::NodeMetadata,
        _ctx: &mut dyn CodecContext,
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
        _ctx: &dyn CodecContext,
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
        let type_system = Arc::new(lsp::type_system::JavaTypeSystem::new());
        Ok(Self {
            parser,
            resolver,
            type_system,
        })
    }
}

impl FileMatcherCap for JavaPlugin {
    fn supports_path(&self, path: &Path) -> bool {
        path.extension()
            .and_then(|e| e.to_str())
            .map(|ext| ext.eq_ignore_ascii_case("java"))
            .unwrap_or(false)
    }
}

impl LanguageParseCap for JavaPlugin {
    fn parse_language_file(
        &self,
        source: &str,
        path: &Path,
    ) -> std::result::Result<naviscope_plugin::GlobalParseResult, naviscope_plugin::BoxError> {
        self.parser.parse_file(source, Some(path))
    }
}

impl SymbolResolveService for JavaPlugin {
    fn resolve_at(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
        line: usize,
        byte_col: usize,
        index: &dyn naviscope_plugin::CodeGraph,
    ) -> Option<naviscope_api::models::SymbolResolution> {
        self.resolver.resolve_at(tree, source, line, byte_col, index)
    }
}

impl SymbolQueryService for JavaPlugin {
    fn find_matches(
        &self,
        index: &dyn naviscope_plugin::CodeGraph,
        res: &naviscope_api::models::SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::FqnId> {
        self.resolver.find_matches(index, res)
    }

    fn resolve_type_of(
        &self,
        index: &dyn naviscope_plugin::CodeGraph,
        res: &naviscope_api::models::SymbolResolution,
    ) -> Vec<naviscope_api::models::SymbolResolution> {
        self.resolver.resolve_type_of(index, res)
    }

    fn find_implementations(
        &self,
        index: &dyn naviscope_plugin::CodeGraph,
        res: &naviscope_api::models::SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::FqnId> {
        self.resolver.find_implementations(index, res)
    }
}

impl LspSyntaxService for JavaPlugin {
    fn parse(&self, source: &str, old_tree: Option<&tree_sitter::Tree>) -> Option<tree_sitter::Tree> {
        crate::lsp::JavaLspService::new(self.parser.clone()).parse(source, old_tree)
    }

    fn extract_symbols(
        &self,
        tree: &tree_sitter::Tree,
        source: &str,
    ) -> Vec<naviscope_api::models::graph::DisplayGraphNode> {
        crate::lsp::JavaLspService::new(self.parser.clone()).extract_symbols(tree, source)
    }

    fn find_occurrences(
        &self,
        source: &str,
        tree: &tree_sitter::Tree,
        target: &naviscope_api::models::SymbolResolution,
    ) -> Vec<naviscope_api::models::symbol::Range> {
        crate::lsp::JavaLspService::new(self.parser.clone())
            .find_occurrences(source, tree, target)
    }
}

impl ReferenceCheckService for JavaPlugin {
    fn is_reference_to(
        &self,
        graph: &dyn naviscope_plugin::CodeGraph,
        candidate: &naviscope_api::models::SymbolResolution,
        target: &naviscope_api::models::SymbolResolution,
    ) -> bool {
        self.type_system.is_reference_to(graph, candidate, target)
    }

    fn is_subtype(&self, graph: &dyn naviscope_plugin::CodeGraph, sub: &str, sup: &str) -> bool {
        self.type_system.is_subtype(graph, sub, sup)
    }
}

impl SourceIndexCap for JavaPlugin {
    fn compile_source(
        &self,
        file: &ParsedFile,
        context: &ProjectContext,
    ) -> std::result::Result<ResolvedUnit, naviscope_plugin::BoxError> {
        self.resolver.compile_source(file, context)
    }
}

impl AssetCap for JavaPlugin {
    fn global_asset_discoverer(&self) -> Option<Box<dyn AssetDiscoverer>> {
        Some(Box::new(crate::discoverer::JdkDiscoverer::new()))
    }

    fn asset_indexer(&self) -> Option<Arc<dyn AssetIndexer>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
    }

    fn asset_source_locator(&self) -> Option<Arc<dyn AssetSourceLocator>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
    }

    fn stub_generator(&self) -> Option<Arc<dyn naviscope_plugin::StubGenerator>> {
        Some(Arc::new(crate::resolver::external::JavaExternalResolver))
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

impl MetadataCodecCap for JavaPlugin {
    fn metadata_codec(&self) -> Option<Arc<dyn NodeMetadataCodec>> {
        Some(Arc::new(self.clone()))
    }
}

pub fn java_caps() -> std::result::Result<LanguageCaps, Box<dyn std::error::Error + Send + Sync>> {
    let plugin = Arc::new(JavaPlugin::new()?);
    Ok(LanguageCaps {
        language: Language::JAVA,
        matcher: plugin.clone(),
        parser: plugin.clone(),
        semantic: plugin.clone() as Arc<dyn SemanticCap>,
        indexing: plugin.clone(),
        asset: plugin.clone(),
        presentation: plugin.clone(),
        metadata_codec: plugin,
    })
}
