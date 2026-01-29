use crate::parser::SymbolIntent;
use crate::parser::SymbolResolution;
use crate::parser::java::JavaParser;
use crate::resolver::lang::java::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;

pub struct BuiltinScope<'a> {
    pub parser: &'a JavaParser,
}

impl SemanticScope<ResolutionContext<'_>> for BuiltinScope<'_> {
    fn resolve(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<Result<SymbolResolution, ()>> {
        if context.intent != SymbolIntent::Type {
            return None;
        }

        self.parser
            .resolve_type_name_to_fqn_data(name, context.package.as_deref(), &context.imports)
            .and_then(|fqn| {
                // Only return if it's a known FQN or a primitive or java.lang
                if context.index.fqn_map.contains_key(&fqn)
                    || fqn.starts_with("java.lang.")
                    || !fqn.contains('.')
                {
                    Some(Ok(SymbolResolution::Precise(fqn, SymbolIntent::Type)))
                } else {
                    None
                }
            })
    }
    fn name(&self) -> &'static str {
        "Builtin"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::CodeGraph;
    use tree_sitter::Parser;

    #[test]
    fn test_builtin_scope_java_lang() {
        let source = "class Test { String s; }";
        let mut parser = Parser::new();
        parser
            .set_language(&crate::parser::java::JavaParser::new().unwrap().language)
            .expect("Error loading Java grammar");
        let tree = parser.parse(source, None).unwrap();

        let string_node = tree
            .root_node()
            .named_descendant_for_point_range(
                tree_sitter::Point::new(0, 13),
                tree_sitter::Point::new(0, 19),
            )
            .unwrap();

        let java_parser = JavaParser::new().unwrap();
        let index = CodeGraph::new();

        let context = ResolutionContext::new(
            string_node,
            "String".to_string(),
            &index,
            source,
            &tree,
            &java_parser,
        );

        let scope = BuiltinScope {
            parser: &java_parser,
        };
        let res = scope.resolve("String", &context);

        assert!(res.is_some());
        match res.unwrap() {
            Ok(SymbolResolution::Precise(fqn, _)) => {
                assert_eq!(fqn, "java.lang.String");
            }
            _ => panic!("Expected Precise resolution"),
        }
    }
}
