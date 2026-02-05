use crate::model::{JavaIndexMetadata, JavaNodeMetadata};
use crate::parser::JavaParser;
use crate::resolver::context::ResolutionContext;
use crate::resolver::scope::SemanticScope;
use naviscope_api::models::{SymbolResolution, TypeRef};

pub struct MemberScope<'a> {
    pub parser: &'a JavaParser,
}

impl MemberScope<'_> {
    fn resolve_type_ref_fqns(&self, type_ref: &TypeRef, context: &ResolutionContext) -> TypeRef {
        match type_ref {
            TypeRef::Raw(name) | TypeRef::Id(name) => {
                if let Some(fqn) =
                    self.parser
                        .resolve_type_name_to_fqn(name, context.tree, context.source)
                {
                    TypeRef::Id(fqn)
                } else {
                    TypeRef::Raw(name.clone())
                }
            }
            TypeRef::Generic { base, args } => TypeRef::Generic {
                base: Box::new(self.resolve_type_ref_fqns(base, context)),
                args: args
                    .iter()
                    .map(|a| self.resolve_type_ref_fqns(a, context))
                    .collect(),
            },
            TypeRef::Array {
                element,
                dimensions,
            } => TypeRef::Array {
                element: Box::new(self.resolve_type_ref_fqns(element, context)),
                dimensions: *dimensions,
            },
            TypeRef::Wildcard {
                bound,
                is_upper_bound,
            } => TypeRef::Wildcard {
                bound: bound
                    .as_ref()
                    .map(|b| Box::new(self.resolve_type_ref_fqns(b, context))),
                is_upper_bound: *is_upper_bound,
            },
            _ => type_ref.clone(),
        }
    }

    fn get_base_fqn(&self, type_ref: &TypeRef) -> Option<String> {
        match type_ref {
            TypeRef::Id(s) | TypeRef::Raw(s) => Some(s.clone()),
            TypeRef::Generic { base, .. } => self.get_base_fqn(base),
            _ => None,
        }
    }

    fn resolve_fqn_from_context(&self, name: &str, context: &ResolutionContext) -> Option<String> {
        let lookup_index = |n: &str| -> bool { !context.index.resolve_fqn(n).is_empty() };

        // 1. Check if it's already an FQN in the index or current unit
        if lookup_index(name)
            || context
                .unit
                .map_or(false, |u| u.nodes.contains_key(&name.into()))
        {
            return Some(name.to_string());
        }

        // 2. Check inner classes in enclosing classes
        for container_fqn in &context.enclosing_classes {
            let candidate = format!("{}.{}", container_fqn, name);
            if lookup_index(&candidate)
                || context
                    .unit
                    .map_or(false, |u| u.nodes.contains_key(&candidate.as_str().into()))
            {
                return Some(candidate);
            }
        }

        // 3. Use parser's resolution (imports/package)
        if let Some(fqn) = self
            .parser
            .resolve_type_name_to_fqn(name, context.tree, context.source)
        {
            if fqn != name {
                return Some(fqn);
            }
        }

        Some(name.to_string())
    }

    fn resolve_expression_type(
        &self,
        node: &tree_sitter::Node,
        context: &ResolutionContext,
    ) -> Option<TypeRef> {
        // Helper to get node from index by string FQN
        let get_index_node = |fqn: &str| -> Option<naviscope_api::models::graph::GraphNode> {
            context
                .index
                .resolve_fqn(fqn)
                .into_iter()
                .next()
                .and_then(|id| context.index.get_node(id))
        };

        let kind = node.kind();
        // println!("RESOLVE_EXPR: kind={}, text={:?}", kind, node.utf8_text(context.source.as_bytes()).unwrap_or(""));

        match kind {
            "identifier" | "type_identifier" => {
                let name = node.utf8_text(context.source.as_bytes()).ok()?;
                // 1. Local Scope
                if let Some((_, maybe_type_node)) =
                    self.parser
                        .find_local_declaration_node(*node, name, context.source)
                {
                    if let Some(type_node) = maybe_type_node {
                        // Parse the type node properly to handle generics
                        let type_ref = self.parser.parse_type_node(type_node, context.source);

                        // Resolve FQNs within the parsed type ref
                        let resolved_type_ref = self.resolve_type_ref_fqns(&type_ref, context);
                        return Some(resolved_type_ref);
                    }

                    // Heuristic: Try to infer lambda parameter type
                    return self.infer_lambda_param_type(node, context);
                }
                // 2. Lexical Scope
                for container_fqn in &context.enclosing_classes {
                    let candidate = format!("{}.{}", container_fqn, name);

                    // Check index
                    if let Some(node) = get_index_node(&candidate) {
                        if let JavaNodeMetadata::Field { type_ref, .. } =
                            node.metadata.as_any().downcast_ref::<JavaNodeMetadata>()?
                        {
                            return Some(type_ref.clone());
                        }
                        return Some(TypeRef::Id(candidate));
                    }

                    // Check current unit (indexing phase)
                    if let Some(unit) = context.unit {
                        if let Some(node) = unit.nodes.get(&candidate.as_str().into()) {
                            if let Some(java_meta) =
                                node.metadata.as_any().downcast_ref::<JavaIndexMetadata>()
                            {
                                if let JavaIndexMetadata::Field { type_ref, .. } = java_meta {
                                    return Some(type_ref.clone());
                                }
                            }
                            return Some(TypeRef::Id(candidate));
                        }
                    }
                }
                // 3. Global Scope (Check if it's a known class FQN in the index or unit)
                let fqn =
                    self.parser
                        .resolve_type_name_to_fqn(name, context.tree, context.source)?;

                // If it's a known class, return it.
                // Check index presence
                let in_index = !context.index.resolve_fqn(&fqn).is_empty();

                if in_index
                    || context
                        .unit
                        .map_or(false, |u| u.nodes.contains_key(&fqn.as_str().into()))
                {
                    return Some(TypeRef::Id(fqn.clone()));
                }

                // Fallback: maybe it's a package or a class not yet in index but resolvable via imports
                Some(TypeRef::Id(fqn))
            }
            "field_access" => {
                let receiver = node.child_by_field_name("object")?;
                let field_name = node
                    .child_by_field_name("field")?
                    .utf8_text(context.source.as_bytes())
                    .ok()?;

                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let raw_receiver_type = self.get_base_fqn(&receiver_type_ref)?;
                let receiver_type = self.resolve_fqn_from_context(&raw_receiver_type, context)?;
                let field_fqn = format!("{}.{}", receiver_type, field_name);

                // Check index
                if let Some(node) = get_index_node(&field_fqn) {
                    if let Some(java_meta) =
                        node.metadata.as_any().downcast_ref::<JavaNodeMetadata>()
                    {
                        if let JavaNodeMetadata::Field { type_ref, .. } = java_meta {
                            return Some(type_ref.clone());
                        }
                    }
                }

                // Check unit
                if let Some(unit) = context.unit {
                    if let Some(node) = unit.nodes.get(&field_fqn.as_str().into()) {
                        if let Some(java_meta) =
                            node.metadata.as_any().downcast_ref::<JavaIndexMetadata>()
                        {
                            if let JavaIndexMetadata::Field { type_ref, .. } = java_meta {
                                return Some(type_ref.clone());
                            }
                        }
                    }
                }
                None
            }
            "method_invocation" => {
                let receiver = node.child_by_field_name("object")?;
                let method_name = node
                    .child_by_field_name("name")?
                    .utf8_text(context.source.as_bytes())
                    .ok()?;

                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let raw_receiver_type = self.get_base_fqn(&receiver_type_ref)?;
                let receiver_type = self.resolve_fqn_from_context(&raw_receiver_type, context)?;
                let method_fqn = format!("{}.{}", receiver_type, method_name);

                // Check index
                if let Some(node) = get_index_node(&method_fqn) {
                    if let Some(java_meta) =
                        node.metadata.as_any().downcast_ref::<JavaNodeMetadata>()
                    {
                        if let JavaNodeMetadata::Method { return_type, .. } = java_meta {
                            return Some(return_type.clone());
                        }
                    }
                }

                // Check unit
                if let Some(unit) = context.unit {
                    if let Some(node) = unit.nodes.get(&method_fqn.as_str().into()) {
                        if let Some(java_meta) =
                            node.metadata.as_any().downcast_ref::<JavaIndexMetadata>()
                        {
                            if let JavaIndexMetadata::Method { return_type, .. } = java_meta {
                                return Some(return_type.clone());
                            }
                        }
                    }
                }
                None
            }
            "this" => context
                .enclosing_classes
                .first()
                .map(|s| TypeRef::Id(s.clone())),
            "scoped_type_identifier" | "scoped_identifier" => {
                let receiver = node.child_by_field_name("scope")?;
                let name = node
                    .child_by_field_name("name")?
                    .utf8_text(context.source.as_bytes())
                    .ok()?;
                let receiver_type_ref = self.resolve_expression_type(&receiver, context)?;
                let receiver_type = self.get_base_fqn(&receiver_type_ref)?;
                Some(TypeRef::Id(format!("{}.{}", receiver_type, name)))
            }
            _ => None,
        }
    }

    fn infer_lambda_param_type(
        &self,
        node: &tree_sitter::Node,
        context: &ResolutionContext,
    ) -> Option<TypeRef> {
        let mut curr = *node;
        while let Some(parent) = curr.parent() {
            if parent.kind() == "lambda_expression" {
                return self.resolve_lambda_type_from_parent(&parent, context);
            }
            curr = parent;
        }
        None
    }

    fn resolve_lambda_type_from_parent(
        &self,
        lambda_node: &tree_sitter::Node,
        context: &ResolutionContext,
    ) -> Option<TypeRef> {
        let invocation = lambda_node
            .parent()
            .filter(|n| n.kind() == "argument_list")?;
        let method_call = invocation
            .parent()
            .filter(|n| n.kind() == "method_invocation")?;

        let method_name = method_call
            .child_by_field_name("name")?
            .utf8_text(context.source.as_bytes())
            .ok()?;

        if !matches!(
            method_name,
            "forEach" | "filter" | "map" | "anyMatch" | "allMatch"
        ) {
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
    fn resolve(
        &self,
        name: &str,
        context: &ResolutionContext,
    ) -> Option<Result<SymbolResolution, ()>> {
        if name == "this" {
            return context
                .enclosing_classes
                .first()
                .cloned()
                .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)));
        }

        context
            .receiver_node
            .as_ref()
            .map(|recv| {
                // Case A: Explicit Receiver (obj.field)
                self.resolve_expression_type(recv, context)
                    .and_then(|type_ref| self.get_base_fqn(&type_ref))
                    .and_then(|raw_type_fqn| self.resolve_fqn_from_context(&raw_type_fqn, context))
                    .map(|type_fqn| format!("{}#{}", type_fqn, name))
                    .and_then(|candidate| {
                        context
                            .index
                            .resolve_fqn(&candidate)
                            .into_iter()
                            .next()
                            .map(|id| {
                                use naviscope_plugin::NamingConvention;
                                crate::naming::JavaNamingConvention
                                    .render_fqn(id, context.index.fqns())
                            })
                    })
                    .map(|fqn| Ok(SymbolResolution::Precise(fqn, context.intent)))
                    .unwrap_or(Err(()))
            })
            .or_else(|| {
                // Case B: Implicit this (Lexical Scope)
                context
                    .enclosing_classes
                    .iter()
                    .map(|container_fqn| format!("{}#{}", container_fqn, name))
                    .find_map(|candidate| {
                        context
                            .index
                            .resolve_fqn(&candidate)
                            .into_iter()
                            .next()
                            .map(|id| {
                                use naviscope_plugin::NamingConvention;
                                let fqn = crate::naming::JavaNamingConvention
                                    .render_fqn(id, context.index.fqns());
                                Ok(SymbolResolution::Precise(fqn, context.intent))
                            })
                    })
            })
    }
    fn name(&self) -> &'static str {
        "Member"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_api::models::graph::{EdgeType, GraphNode};
    use naviscope_api::models::symbol::{FqnId, FqnNode, FqnReader, Symbol};
    use naviscope_plugin::{CodeGraph, Direction};
    use std::path::Path;

    struct MockCodeGraph {
        node: GraphNode,
    }

    impl CodeGraph for MockCodeGraph {
        fn resolve_fqn(&self, _fqn: &str) -> Vec<FqnId> {
            vec![FqnId(0)]
        }
        fn get_node_at(&self, _path: &Path, _line: usize, _col: usize) -> Option<FqnId> {
            None
        }
        fn resolve_atom(&self, _atom: Symbol) -> &str {
            ""
        }
        fn fqns(&self) -> &dyn FqnReader {
            self
        }
        fn get_node(&self, _id: FqnId) -> Option<GraphNode> {
            Some(self.node.clone())
        }
        fn get_neighbors(
            &self,
            _id: FqnId,
            _direction: Direction,
            _edge_type: Option<EdgeType>,
        ) -> Vec<FqnId> {
            vec![]
        }
    }

    impl FqnReader for MockCodeGraph {
        fn resolve_node(&self, _id: FqnId) -> Option<FqnNode> {
            None
        }
        fn resolve_atom(&self, _atom: Symbol) -> &str {
            ""
        }
    }

    use tree_sitter::Parser;

    #[test]
    fn test_member_scope_implicit_this() {
        let source = "class Test { int field; void main() { field = 1; } }";
        let mut parser = Parser::new();
        parser
            .set_language(&crate::parser::JavaParser::new().unwrap().language)
            .expect("Error loading Java grammar");
        let tree = parser.parse(source, None).unwrap();

        // Find the 'field' in field = 1
        let field_node = tree
            .root_node()
            .named_descendant_for_point_range(
                tree_sitter::Point::new(0, 38),
                tree_sitter::Point::new(0, 43),
            )
            .unwrap();

        let java_parser = JavaParser::new().unwrap();

        let index = MockCodeGraph {
            node: naviscope_api::models::graph::GraphNode {
                id: FqnId(0),
                name: Symbol(lasso::Spur::default()),
                kind: naviscope_api::models::graph::NodeKind::Field,
                lang: Symbol(lasso::Spur::default()),
                source: naviscope_api::models::graph::NodeSource::Project,
                location: None,
                metadata: std::sync::Arc::new(JavaNodeMetadata::Field {
                    type_ref: naviscope_api::models::TypeRef::Raw("int".to_string()),
                    modifiers_sids: vec![],
                }),
            },
        };

        let context = ResolutionContext::new(
            field_node,
            "field".to_string(),
            &index,
            source,
            &tree,
            &java_parser,
        );

        let scope = MemberScope {
            parser: &java_parser,
        };
        let res = scope.resolve("field", &context);

        assert!(res.is_some());
    }
}
