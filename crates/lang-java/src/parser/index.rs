use super::JavaParser;
use naviscope_api::models::graph::{DisplaySymbolLocation, NodeKind, ResolutionStatus};
use naviscope_plugin::utils::range_from_ts;
use naviscope_plugin::{GlobalParseResult, IndexNode, IndexRelation, ParseOutput};
use std::sync::Arc;
use tree_sitter::Parser;

type GenericResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

impl JavaParser {
    pub fn parse_file(
        &self,
        source_code: &str,
        file_path: Option<&std::path::Path>,
    ) -> GenericResult<GlobalParseResult> {
        let mut parser = Parser::new();
        parser
            .set_language(&self.language)
            .map_err(|e| format!("Failed to set language: {}", e))?;

        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| "Failed to parse Java file")?;

        // Use the native AST analyzer
        let model = self.analyze(&tree, source_code);

        let nodes: Vec<IndexNode> = model
            .entities
            .into_iter()
            .map(|e| {
                let kind = match &e.element {
                    crate::model::JavaIndexMetadata::Class { .. } => NodeKind::Class,
                    crate::model::JavaIndexMetadata::Interface { .. } => NodeKind::Interface,
                    crate::model::JavaIndexMetadata::Enum { .. } => NodeKind::Enum,
                    crate::model::JavaIndexMetadata::Annotation { .. } => NodeKind::Annotation,
                    crate::model::JavaIndexMetadata::Method { is_constructor, .. } => {
                        if *is_constructor {
                            NodeKind::Constructor
                        } else {
                            NodeKind::Method
                        }
                    }
                    crate::model::JavaIndexMetadata::Field { .. } => NodeKind::Field,
                    crate::model::JavaIndexMetadata::Package => NodeKind::Package,
                };

                let location = file_path.map(|p| DisplaySymbolLocation {
                    path: p.to_string_lossy().to_string(),
                    range: range_from_ts(e.node.range()),
                    selection_range: e
                        .node
                        .child_by_field_name("name")
                        .map(|n| range_from_ts(n.range())),
                });

                IndexNode {
                    id: e.fqn.clone(),
                    name: e.name.clone(),
                    kind,
                    lang: "java".to_string(),
                    source: naviscope_api::models::graph::NodeSource::Project,
                    status: ResolutionStatus::Resolved,
                    location,
                    metadata: Arc::new(e.element),
                }
            })
            .collect();

        let relations: Vec<IndexRelation> = model
            .relations
            .into_iter()
            .map(|r| IndexRelation {
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
