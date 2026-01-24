use crate::parser::java::JavaParser;
use crate::parser::SymbolResolution;
use crate::model::lang::java::JavaElement;
use crate::model::graph::GraphNode;
use crate::model::signature::TypeRef;
use crate::resolver::lang::java::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;

pub struct MemberScope<'a> {
    pub parser: &'a JavaParser,
}

impl MemberScope<'_> {
    fn get_base_fqn(&self, type_ref: &TypeRef) -> Option<String> {
        match type_ref {
            TypeRef::Id(s) | TypeRef::Raw(s) => Some(s.clone()),
            TypeRef::Generic { base, .. } => self.get_base_fqn(base),
            _ => None,
        }
    }

    fn resolve_expression_type(&self, node: &tree_sitter::Node, context: &ResolutionContext) -> Option<TypeRef> {
        let kind = node.kind();
        match kind {
            "identifier" | "type_identifier" => {
                let name = node.utf8_text(context.source.as_bytes()).ok()?;
                // 1. Local Scope
                if let Some((_, maybe_type)) = self.parser.find_local_declaration(*node, name, context.source) {
                    if let Some(type_name) = maybe_type {
                        if let Some(fqn) = self.parser.resolve_type_name_to_fqn(&type_name, context.tree, context.source) {
                            return Some(TypeRef::Id(fqn));
                        }
                        return Some(TypeRef::Raw(type_name));
                    }
                    
                    // Heuristic: Try to infer lambda parameter type
                    return self.infer_lambda_param_type(node, context);
                }
                // 2. Lexical Scope
                for container_fqn in &context.enclosing_classes {
                    let candidate = format!("{}.{}", container_fqn, name);
                    if context.index.fqn_map.contains_key(&candidate) {
                        return Some(TypeRef::Id(candidate));
                    }
                }
                // 3. Global Scope (Check if it's a known class FQN in the index)
                let fqn = self.parser.resolve_type_name_to_fqn(name, context.tree, context.source)?;
                
                // If it's a known class, return it.
                if context.index.fqn_map.contains_key(&fqn) {
                    return Some(TypeRef::Id(fqn.clone()));
                }
                
                // Fallback: maybe it's a package or a class not yet in index but resolvable via imports
                Some(TypeRef::Id(fqn))
            }
            "field_access" => {
                let receiver = node.child_by_field_name("object")?;
                let field_name = node.child_by_field_name("field")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let receiver_type = self.get_base_fqn(&receiver_type_ref)?;
                
                let field_fqn = format!("{}.{}", receiver_type, field_name);
                
                if let Some(&idx) = context.index.fqn_map.get(&field_fqn) {
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Field(f), .. }) = &context.index.topology[idx] {
                        return Some(f.type_ref.clone());
                    }
                }
                None
            }
            "method_invocation" => {
                let receiver = node.child_by_field_name("object")?;
                let method_name = node.child_by_field_name("name")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let receiver_type = self.get_base_fqn(&receiver_type_ref)?;

                let method_fqn = format!("{}.{}", receiver_type, method_name);
                if let Some(&idx) = context.index.fqn_map.get(&method_fqn) {
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Method(m), .. }) = &context.index.topology[idx] {
                        return Some(m.return_type.clone());
                    }
                }
                None
            }
            "this" => context.enclosing_classes.first().map(|s| TypeRef::Id(s.clone())),
            "scoped_type_identifier" | "scoped_identifier" => {
                let receiver = node.child_by_field_name("scope")?;
                let name = node.child_by_field_name("name")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let receiver_type = self.get_base_fqn(&receiver_type_ref)?;
                Some(TypeRef::Id(format!("{}.{}", receiver_type, name)))
            }
            _ => None
        }
    }

    fn infer_lambda_param_type(&self, node: &tree_sitter::Node, context: &ResolutionContext) -> Option<TypeRef> {
        let mut curr = *node;
        while let Some(parent) = curr.parent() {
            if parent.kind() == "lambda_expression" {
                return self.resolve_lambda_type_from_parent(&parent, context);
            }
            curr = parent;
        }
        None
    }

    fn resolve_lambda_type_from_parent(&self, lambda_node: &tree_sitter::Node, context: &ResolutionContext) -> Option<TypeRef> {
        let invocation = lambda_node.parent().filter(|n| n.kind() == "argument_list")?;
        let method_call = invocation.parent().filter(|n| n.kind() == "method_invocation")?;

        let method_name = method_call
            .child_by_field_name("name")?
            .utf8_text(context.source.as_bytes())
            .ok()?;

        if !matches!(method_name, "forEach" | "filter" | "map" | "anyMatch" | "allMatch") {
            return None;
        }

        let receiver = method_call.child_by_field_name("object")?;
        let receiver_type = self.resolve_expression_type(&receiver, context)?;

        self.extract_first_generic_arg(&receiver_type)
    }

    fn extract_first_generic_arg(&self, type_ref: &TypeRef) -> Option<TypeRef> {
        if let TypeRef::Generic { args, .. } = type_ref {
            args.first().cloned()
        } else {
            None
        }
    }
}

impl SemanticScope<ResolutionContext<'_>> for MemberScope<'_> {
    fn resolve(&self, name: &str, context: &ResolutionContext) -> Option<Result<SymbolResolution, ()>> {
        if name == "this" {
            return context.enclosing_classes.first().cloned()
                .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)));
        }
        context.receiver_node
            .map(|recv| {
                // Case A: Explicit Receiver (obj.field)
                self.resolve_expression_type(&recv, context)
                    .and_then(|type_ref| self.get_base_fqn(&type_ref))
                    .map(|type_fqn| format!("{}.{}", type_fqn, name))
                    .and_then(|candidate| context.index.fqn_map.contains_key(&candidate).then_some(candidate))
                    .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)))
                    .unwrap_or(Err(()))
            })
            .or_else(|| {
                // Case B: Implicit this (Lexical Scope)
                context.enclosing_classes.iter()
                    .map(|container_fqn| format!("{}.{}", container_fqn, name))
                    .find(|candidate| context.index.fqn_map.contains_key(candidate))
                    .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)))
            })
    }
    fn name(&self) -> &'static str { "Member" }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::CodeGraph;
    use tree_sitter::Parser;

    #[test]
    fn test_member_scope_implicit_this() {
        let source = "class Test { int field; void main() { field = 1; } }";
        let mut parser = Parser::new();
        parser.set_language(&crate::parser::java::JavaParser::new().unwrap().language).expect("Error loading Java grammar");
        let tree = parser.parse(source, None).unwrap();
        
        // Find the 'field' in field = 1
        let field_node = tree.root_node().named_descendant_for_point_range(
            tree_sitter::Point::new(0, 38),
            tree_sitter::Point::new(0, 43)
        ).unwrap();
        
        let java_parser = JavaParser::new().unwrap();
        let mut index = CodeGraph::new();
        // Register Test.field in index
        index.fqn_map.insert("Test.field".to_string(), petgraph::graph::NodeIndex::new(0));

        let context = ResolutionContext::new(
            field_node,
            "field".to_string(),
            &index,
            source,
            &tree,
            &java_parser,
        );

        let scope = MemberScope { parser: &java_parser };
        let res = scope.resolve("field", &context);
        
        assert!(res.is_some());
        match res.unwrap() {
            Ok(SymbolResolution::Precise(fqn, _intent)) => {
                assert_eq!(fqn, "Test.field");
                // The intent check might fail because determine_intent relies on the parent node context
                // which might be different in this isolated test string
            }
            _ => panic!("Expected Precise resolution"),
        }
    }
}
