use super::JavaParser;
use naviscope_core::engine::storage::GLOBAL_POOL;
use naviscope_core::error::{NaviscopeError, Result};
use naviscope_core::model::{GraphNode, NodeLocation};
use naviscope_core::parser::{GlobalParseResult, IndexParser};
use smol_str::SmolStr;
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

        let nodes = model
            .entities
            .into_iter()
            .map(|e| {
                let kind = match &e.element {
                    crate::model::JavaElement::Class(_) => naviscope_core::model::NodeKind::Class,
                    crate::model::JavaElement::Interface(_) => {
                        naviscope_core::model::NodeKind::Interface
                    }
                    crate::model::JavaElement::Enum(_) => naviscope_core::model::NodeKind::Enum,
                    crate::model::JavaElement::Annotation(_) => {
                        naviscope_core::model::NodeKind::Annotation
                    }
                    crate::model::JavaElement::Method(m) => {
                        if m.is_constructor {
                            naviscope_core::model::NodeKind::Constructor
                        } else {
                            naviscope_core::model::NodeKind::Method
                        }
                    }
                    crate::model::JavaElement::Field(_) => naviscope_core::model::NodeKind::Field,
                    crate::model::JavaElement::Package(_) => {
                        naviscope_core::model::NodeKind::Package
                    }
                };

                let location = file_path.map(|p| NodeLocation {
                    path: GLOBAL_POOL.intern_path(p),
                    range: naviscope_core::parser::utils::range_from_ts(e.node.range()),
                    selection_range: e.node.child_by_field_name("name").map(|n| naviscope_core::parser::utils::range_from_ts(n.range())),
                });

                GraphNode {
                    id: Arc::from(e.fqn.as_str()),
                    name: SmolStr::from(e.name.as_str()),
                    kind,
                    lang: Arc::from("java"),
                    location,
                    metadata: serde_json::to_value(&e.element).unwrap_or(serde_json::Value::Null),
                }
            })
            .collect();

        let relations = model
            .relations
            .into_iter()
            .map(|r| (r.source_fqn, r.target_name, r.rel_type, r.range))
            .collect();

        let package_name = model.package;
        let imports = model.imports;
        let identifiers = model.identifiers;

        Ok(GlobalParseResult {
            package_name,
            imports,
            nodes,
            relations,
            source: Some(source_code.to_string()),
            tree: Some(tree),
            identifiers,
        })
    }
}
