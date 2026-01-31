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
            .map(|e| GraphNode::java(e.element, file_path.map(|p| p.to_path_buf())))
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
