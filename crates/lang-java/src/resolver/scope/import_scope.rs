use crate::parser::JavaParser;
use crate::resolver::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;

use naviscope_core::ingest::parser::SymbolResolution;

pub struct ImportScope<'a> {
    pub parser: &'a JavaParser,
}

impl SemanticScope<ResolutionContext<'_>> for ImportScope<'_> {
    fn resolve(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<Result<SymbolResolution, ()>> {
        // 1. Precise imports
        context
            .imports
            .iter()
            .find(|imp| imp.ends_with(&format!(".{}", name)))
            .map(|imp| Ok(SymbolResolution::Precise(imp.clone(), context.intent)))
            .or_else(|| {
                // 2. Current package
                context
                    .package
                    .as_ref()
                    .map(|pkg| format!("{}.{}", pkg, name))
                    .and_then(|candidate| {
                        if context.index.find_node(&candidate).is_some() {
                            Some(candidate)
                        } else {
                            None
                        }
                    })
                    .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)))
            })
    }
    fn name(&self) -> &'static str {
        "Import"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_core::model::CodeGraph;
    use tree_sitter::Parser;

    #[test]
    fn test_import_scope_precise() {
        let source = "import java.util.List; class Test { List x; }";
        let mut parser = Parser::new();
        parser
            .set_language(&crate::parser::JavaParser::new().unwrap().language)
            .expect("Error loading Java grammar");
        let tree = parser.parse(source, None).unwrap();

        let list_node = tree
            .root_node()
            .named_descendant_for_point_range(
                tree_sitter::Point::new(0, 36),
                tree_sitter::Point::new(0, 40),
            )
            .unwrap();

        let java_parser = JavaParser::new().unwrap();
        let index = CodeGraph::empty();

        let context = ResolutionContext::new(
            list_node,
            "List".to_string(),
            &index,
            source,
            &tree,
            &java_parser,
        );

        let scope = ImportScope {
            parser: &java_parser,
        };
        let res = scope.resolve("List", &context);

        assert!(res.is_some());
        match res.unwrap() {
            Ok(SymbolResolution::Precise(fqn, _)) => {
                assert_eq!(fqn, "java.util.List");
            }
            _ => panic!("Expected Precise resolution"),
        }
    }
}
