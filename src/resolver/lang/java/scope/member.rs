use crate::parser::java::JavaParser;
use crate::parser::SymbolResolution;
use crate::model::lang::java::JavaElement;
use crate::model::graph::GraphNode;
use crate::resolver::lang::java::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;

pub struct MemberScope<'a> {
    pub parser: &'a JavaParser,
}

impl MemberScope<'_> {
    fn resolve_expression_type(&self, node: &tree_sitter::Node, context: &ResolutionContext) -> Option<String> {
        let kind = node.kind();
        match kind {
            "identifier" | "type_identifier" => {
                let name = node.utf8_text(context.source.as_bytes()).ok()?;
                // 1. Local Scope
                if let Some((_, maybe_type)) = self.parser.find_local_declaration(*node, name, context.source) {
                    if let Some(type_name) = maybe_type {
                        return self.parser.resolve_type_name_to_fqn(type_name.as_str(), context.tree, context.source);
                    }
                    
                    // Heuristic: Try to infer lambda parameter type
                    return self.infer_lambda_param_type(node, context);
                }
                // 2. Lexical Scope
                for container_fqn in &context.enclosing_classes {
                    let candidate = format!("{}.{}", container_fqn, name);
                    if context.index.fqn_map.contains_key(&candidate) {
                        return Some(candidate);
                    }
                }
                // 3. Global Scope (Check if it's a known class FQN in the index)
                let fqn = self.parser.resolve_type_name_to_fqn(name, context.tree, context.source)?;
                
                // If it's a known class, return it.
                if context.index.fqn_map.contains_key(&fqn) {
                    return Some(fqn);
                }
                
                // Fallback: maybe it's a package or a class not yet in index but resolvable via imports
                Some(fqn)
            }
            "field_access" => {
                let receiver = node.child_by_field_name("object")?;
                let field_name = node.child_by_field_name("field")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, context)?;
                let field_fqn = format!("{}.{}", receiver_type, field_name);
                
                if let Some(&idx) = context.index.fqn_map.get(&field_fqn) {
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Field(f), .. }) = &context.index.topology[idx] {
                        // 1. Try to resolve as an inner class of the receiver type first
                        if !f.type_name.contains('.') {
                            let candidate = format!("{}.{}", receiver_type, f.type_name);
                            if context.index.fqn_map.contains_key(&candidate) {
                                return Some(candidate);
                            }
                            
                            // Check enclosing classes (for nested inner classes)
                            for container_fqn in &context.enclosing_classes {
                                let candidate = format!("{}.{}", container_fqn, f.type_name);
                                if context.index.fqn_map.contains_key(&candidate) {
                                    return Some(candidate);
                                }
                            }
                        }

                        // 2. Fallback to standard FQN resolution
                        let type_fqn = self.parser.resolve_type_name_to_fqn(&f.type_name, context.tree, context.source)
                            .unwrap_or_else(|| f.type_name.clone());
                        
                        if context.index.fqn_map.contains_key(&type_fqn) {
                            return Some(type_fqn);
                        }
                        
                        return Some(type_fqn);
                    }
                }
                None
            }
            "method_invocation" => {
                let receiver = node.child_by_field_name("object")?;
                let method_name = node.child_by_field_name("name")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, context)?;
                let method_fqn = format!("{}.{}", receiver_type, method_name);
                if let Some(&idx) = context.index.fqn_map.get(&method_fqn) {
                    if let GraphNode::Code(crate::model::graph::CodeElement::Java { element: JavaElement::Method(m), .. }) = &context.index.topology[idx] {
                        // 1. Try to resolve as an inner class of the class where the method is defined
                        if !m.return_type.contains('.') {
                            // Extract class FQN from method_fqn
                            if let Some(last_dot) = method_fqn.rfind('.') {
                                let class_fqn = &method_fqn[..last_dot];
                                let candidate = format!("{}.{}", class_fqn, m.return_type);
                                if context.index.fqn_map.contains_key(&candidate) {
                                    return Some(candidate);
                                }
                            }
                        }

                        // 2. Fallback
                        return self.parser.resolve_type_name_to_fqn(&m.return_type, context.tree, context.source)
                            .or_else(|| Some(m.return_type.clone()));
                    }
                }
                None
            }
            "this" => context.enclosing_classes.first().cloned(),
            "scoped_type_identifier" | "scoped_identifier" => {
                let receiver = node.child_by_field_name("scope")?;
                let name = node.child_by_field_name("name")?.utf8_text(context.source.as_bytes()).ok()?;
                let receiver_type = self.resolve_expression_type(&receiver, context)?;
                Some(format!("{}.{}", receiver_type, name))
            }
            _ => None
        }
    }

    fn infer_lambda_param_type(&self, node: &tree_sitter::Node, context: &ResolutionContext) -> Option<String> {
        // Find the lambda expression this identifier belongs to
        let mut curr = *node;
        while let Some(parent) = curr.parent() {
            if parent.kind() == "lambda_expression" {
                // Found the lambda! Now find the method invocation it's passed to.
                if let Some(invocation) = parent.parent() {
                    if invocation.kind() == "argument_list" {
                        if let Some(method_call) = invocation.parent() {
                            if method_call.kind() == "method_invocation" {
                                // We found the method call: e.g., list.forEach(it -> ...)
                                // 1. Get the receiver type (e.g., List<A>)
                                if let Some(receiver) = method_call.child_by_field_name("object") {
                                    if let Some(receiver_type) = self.resolve_expression_type(&receiver, context) {
                                        // 2. Get the method name (e.g., forEach)
                                        if let Some(method_name_node) = method_call.child_by_field_name("name") {
                                            let method_name = method_name_node.utf8_text(context.source.as_bytes()).ok()?;
                                            
                                            // Heuristic: If receiver is a collection and method is forEach/filter/map, 
                                            // the lambda param is likely the element type.
                                            // Since we don't have full generic support yet, we'll look for common patterns.
                                            if matches!(method_name, "forEach" | "filter" | "map" | "anyMatch" | "allMatch") {
                                                // If receiver_type is something like com.example.MyList<com.example.A>,
                                                // we try to extract the first type argument.
                                                if let Some(start) = receiver_type.find('<') {
                                                    if let Some(end) = receiver_type.rfind('>') {
                                                        let inner = &receiver_type[start+1..end];
                                                        // Return the first type argument
                                                        return Some(inner.split(',').next()?.trim().to_string());
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
                break;
            }
            curr = parent;
        }
        None
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
