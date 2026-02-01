use crate::parser::JavaParser;
use crate::resolver::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;
use naviscope_core::ingest::parser::SymbolResolution;

pub struct LocalScope<'a> {
    pub parser: &'a JavaParser,
}

impl SemanticScope<ResolutionContext<'_>> for LocalScope<'_> {
    fn resolve(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<Result<SymbolResolution, ()>> {
        // Local scope is only searched if there is no explicit receiver
        if context.receiver_node.is_some() {
            return None;
        }

        self.parser
            .find_local_declaration(context.node, name, context.source)
            .map(|(range, type_name)| Ok(SymbolResolution::Local(range, type_name)))
    }
    fn name(&self) -> &'static str {
        "Local"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_core::model::CodeGraph;
    use tree_sitter::Parser;

    #[test]
    fn test_local_scope_resolve() {
        let source = "class Test { void main() { int x = 1; System.out.println(x); } }";
        let mut parser = Parser::new();
        parser
            .set_language(&crate::parser::JavaParser::new().unwrap().language)
            .expect("Error loading Java grammar");
        let tree = parser.parse(source, None).unwrap();

        // Find the 'x' in println(x)
        let x_node = tree
            .root_node()
            .named_descendant_for_point_range(
                tree_sitter::Point::new(0, 57),
                tree_sitter::Point::new(0, 58),
            )
            .unwrap();

        assert_eq!(x_node.utf8_text(source.as_bytes()).unwrap(), "x");

        let java_parser = JavaParser::new().unwrap();
        let index = CodeGraph::empty();
        let context =
            ResolutionContext::new(x_node, "x".to_string(), &index, source, &tree, &java_parser);

        let scope = LocalScope {
            parser: &java_parser,
        };
        let res = scope.resolve("x", &context);

        assert!(res.is_some());
        match res.unwrap() {
            Ok(SymbolResolution::Local(range, type_name)) => {
                assert_eq!(range.start_line, 0);
                assert_eq!(type_name, Some("int".to_string()));
            }
            _ => panic!("Expected Local resolution"),
        }
    }
}
