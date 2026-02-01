pub mod model;
pub mod parser;
pub mod queries;
pub mod resolver;

use lasso::Key;
use naviscope_api::models::DisplayGraphNode;
use naviscope_core::error::Result;
use naviscope_core::ingest::parser::{GlobalParseResult, LspParser};
use naviscope_core::ingest::resolver::SemanticResolver;
use naviscope_core::model::source::Language;
use naviscope_core::runtime::plugin::{LanguagePlugin, MetadataPlugin, NodeRenderer};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct JavaPlugin {
    parser: Arc<parser::JavaParser>,
    resolver: Arc<resolver::JavaResolver>,
}

impl NodeRenderer for JavaPlugin {
    fn render_display_node(
        &self,
        node: &naviscope_core::model::GraphNode,
        rodeo: &dyn lasso::Reader,
    ) -> DisplayGraphNode {
        let mut display = DisplayGraphNode {
            id: node.fqn(rodeo).to_string(),
            name: node.name(rodeo).to_string(),
            kind: node.kind.clone(),
            lang: "java".to_string(),
            location: node.location.as_ref().map(|l| l.to_display(rodeo)),
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
                            rodeo
                                .resolve(&lasso::Spur::try_from_usize(s as usize).unwrap())
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
                            rodeo
                                .resolve(&lasso::Spur::try_from_usize(s as usize).unwrap())
                                .to_string()
                        })
                        .collect();
                    let params_str = parameters
                        .iter()
                        .map(|p| {
                            format!(
                                "{}: {}",
                                rodeo.resolve(
                                    &lasso::Spur::try_from_usize(p.name_sid as usize).unwrap()
                                ),
                                crate::model::fmt_type(return_type, rodeo)
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
                            crate::model::fmt_type(return_type, rodeo)
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
                            rodeo
                                .resolve(&lasso::Spur::try_from_usize(s as usize).unwrap())
                                .to_string()
                        })
                        .collect();
                    display.signature = Some(format!(
                        "{}: {}",
                        display.name,
                        crate::model::fmt_type(type_ref, rodeo)
                    ));
                }
                crate::model::JavaNodeMetadata::Enum { modifiers_sids, .. } => {
                    display.modifiers = modifiers_sids
                        .iter()
                        .map(|&s| {
                            rodeo
                                .resolve(&lasso::Spur::try_from_usize(s as usize).unwrap())
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

    fn hydrate_display_node(&self, _node: &mut DisplayGraphNode) {
        // Hydration logic is currently disabled as DisplayGraphNode no longer carries raw metadata.
        // LSP symbols are now baked during extraction.
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

impl MetadataPlugin for JavaPlugin {
    fn intern(
        &self,
        metadata: &dyn naviscope_core::model::NodeMetadata,
        ctx: &mut dyn naviscope_core::model::storage::model::StorageContext,
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
            let storage_metadata = java_idx_meta.to_storage(ctx);
            rmp_serde::to_vec(&storage_metadata).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn resolve(
        &self,
        bytes: &[u8],
        _ctx: &dyn naviscope_core::model::storage::model::StorageContext,
    ) -> Arc<dyn naviscope_core::model::NodeMetadata> {
        if let Ok(element) = rmp_serde::from_slice::<crate::model::JavaNodeMetadata>(bytes) {
            Arc::new(element)
        } else {
            Arc::new(naviscope_core::model::EmptyMetadata)
        }
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
        let mut symbols = self.parser.extract_symbols(tree, source);
        for sym in &mut symbols {
            self.hydrate_display_node(sym);
        }
        symbols
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
