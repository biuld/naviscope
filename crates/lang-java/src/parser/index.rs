use super::JavaParser;
use naviscope_core::error::{NaviscopeError, Result};
use naviscope_core::model::graph::GraphNode;
use naviscope_core::parser::{GlobalParseResult, IndexParser};
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
                    crate::model::JavaElement::Class(_) => {
                        naviscope_core::model::graph::NodeKind::Class
                    }
                    crate::model::JavaElement::Interface(_) => {
                        naviscope_core::model::graph::NodeKind::Interface
                    }
                    crate::model::JavaElement::Enum(_) => {
                        naviscope_core::model::graph::NodeKind::Enum
                    }
                    crate::model::JavaElement::Annotation(_) => {
                        naviscope_core::model::graph::NodeKind::Annotation
                    }
                    crate::model::JavaElement::Method(m) => {
                        if m.is_constructor {
                            naviscope_core::model::graph::NodeKind::Constructor
                        } else {
                            naviscope_core::model::graph::NodeKind::Method
                        }
                    }
                    crate::model::JavaElement::Field(_) => {
                        naviscope_core::model::graph::NodeKind::Field
                    }
                    crate::model::JavaElement::Package(_) => {
                        naviscope_core::model::graph::NodeKind::Package
                    }
                };

                let location = file_path.map(|p| naviscope_core::model::graph::NodeLocation {
                    path: p.to_path_buf(),
                    range: e
                        .element
                        .range()
                        .unwrap_or(naviscope_core::model::graph::Range {
                            start_line: 0,
                            start_col: 0,
                            end_line: 0,
                            end_col: 0,
                        }),
                    selection_range: e.element.name_range(),
                });

                GraphNode {
                    id: e.element.id().to_string(),
                    name: e.element.name().to_string(),
                    kind,
                    lang: "java".to_string(),
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
