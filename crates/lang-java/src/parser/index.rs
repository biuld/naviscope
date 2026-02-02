use super::JavaParser;
use naviscope_core::error::{NaviscopeError, Result};
use naviscope_core::ingest::parser::{GlobalParseResult, IndexNode, IndexParser, ParseOutput};
use naviscope_core::model::DisplaySymbolLocation;
use std::sync::Arc;
use tree_sitter::Parser;

impl IndexParser for JavaParser {
    fn parse_file(
        &self,
        source_code: &str,
        file_path: Option<&std::path::Path>,
    ) -> Result<GlobalParseResult> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| NaviscopeError::Parsing(e.to_string()))?;

        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| NaviscopeError::Parsing("Failed to parse Java file".to_string()))?;

        // Use the native AST analyzer
        let model = self.analyze(&tree, source_code);

        let nodes: Vec<naviscope_core::ingest::parser::IndexNode> = model
            .entities
            .into_iter()
            .map(|e| {
                let kind = match &e.element {
                    crate::model::JavaIndexMetadata::Class { .. } => {
                        naviscope_core::model::NodeKind::Class
                    }
                    crate::model::JavaIndexMetadata::Interface { .. } => {
                        naviscope_core::model::NodeKind::Interface
                    }
                    crate::model::JavaIndexMetadata::Enum { .. } => {
                        naviscope_core::model::NodeKind::Enum
                    }
                    crate::model::JavaIndexMetadata::Annotation { .. } => {
                        naviscope_core::model::NodeKind::Annotation
                    }
                    crate::model::JavaIndexMetadata::Method { is_constructor, .. } => {
                        if *is_constructor {
                            naviscope_core::model::NodeKind::Constructor
                        } else {
                            naviscope_core::model::NodeKind::Method
                        }
                    }
                    crate::model::JavaIndexMetadata::Field { .. } => {
                        naviscope_core::model::NodeKind::Field
                    }
                    crate::model::JavaIndexMetadata::Package => {
                        naviscope_core::model::NodeKind::Package
                    }
                };

                let location = file_path.map(|p| DisplaySymbolLocation {
                    path: p.to_string_lossy().to_string(),
                    range: naviscope_core::ingest::parser::utils::range_from_ts(e.node.range()),
                    selection_range: e
                        .node
                        .child_by_field_name("name")
                        .map(|n| naviscope_core::ingest::parser::utils::range_from_ts(n.range())),
                });

                IndexNode {
                    id: e.fqn.clone(),
                    name: e.name.clone(),
                    kind,
                    lang: "java".to_string(),
                    location,
                    metadata: Arc::new(e.element),
                }
            })
            .collect();

        let relations: Vec<naviscope_core::ingest::parser::IndexRelation> = model
            .relations
            .into_iter()
            .map(|r| naviscope_core::ingest::parser::IndexRelation {
                source_id: r.source_id,
                target_id: r.target_id,
                edge_type: r.rel_type,
                range: r.range,
            })
            .collect();

        Ok(GlobalParseResult {
            package_name: model.package,
            imports: model.imports,
            output: ParseOutput {
                nodes,
                relations,
                identifiers: model.identifiers,
            },
            source: Some(source_code.to_string()),
            tree: Some(tree),
        })
    }
}
