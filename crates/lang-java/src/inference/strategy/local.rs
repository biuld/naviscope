//! Local variable type inference.

use super::InferStrategy;
use crate::inference::core::unification::Substitution;
use crate::inference::InferContext;
use crate::inference::TypeRefExt;
use naviscope_api::models::TypeRef;
use tree_sitter::Node;

/// Infer type from local variable declaration.
pub struct LocalVarInfer;

impl InferStrategy for LocalVarInfer {
    fn infer(&self, node: &Node, ctx: &InferContext) -> Option<TypeRef> {
        // Only works for identifier nodes
        if node.kind() != "identifier" {
            return None;
        }

        let name = node.utf8_text(ctx.source.as_bytes()).ok()?;

        // Optimize: If ScopeManager is available, rely on it exclusively.
        // We do NOT fallback to AST walking if lookup fails, because if a ScopeManager
        // is provided, it is expected to be complete. Fallback would only hide bugs.
        //
        // NOTE: If scope_manager is None, we now return None.
        // This forces integrators to use ScopeManager for local variable inference.
        if let Some(sm) = ctx.scope_manager {
            // Find the nearest scope-owning ancestor
            let mut current = *node;
            while let Some(parent) = current.parent() {
                // Check if this parent node owns a scope
                if sm.get_scope_id(parent.id()).is_some() {
                    // Delegate to ScopeManager to lookup variable starting from this scope
                    // The lookup method will automatically traverse up the scope chain
                    if let Some(ty) = sm.lookup(parent.id(), name) {
                        if ty != TypeRef::Unknown {
                            return Some(ty);
                        }
                    }
                    return self.infer_lambda_parameter_type(node, name, ctx);
                }
                current = parent;
            }
            return None;
        }

        None
    }
}

impl LocalVarInfer {
    fn infer_lambda_parameter_type(
        &self,
        node: &Node,
        name: &str,
        ctx: &InferContext,
    ) -> Option<TypeRef> {
        let (lambda, lambda_param_index) = self.find_enclosing_lambda_param(node, name, ctx)?;
        let (call_node, arg_index) = self.find_lambda_call_site(&lambda)?;
        let expected_arg_type =
            self.resolve_expected_argument_type(&call_node, &lambda, arg_index, ctx)?;
        self.extract_lambda_parameter_type(&expected_arg_type, lambda_param_index, ctx)
    }

    fn find_enclosing_lambda_param<'a>(
        &self,
        node: &Node<'a>,
        name: &str,
        ctx: &InferContext,
    ) -> Option<(Node<'a>, usize)> {
        let mut current = *node;
        while let Some(parent) = current.parent() {
            if parent.kind() == "lambda_expression" {
                let params = parent.child_by_field_name("parameters")?;
                let names = collect_lambda_parameter_names(&params, ctx.source);
                if let Some(index) = names.iter().position(|n| n == name) {
                    return Some((parent, index));
                }
            }
            current = parent;
        }
        None
    }

    fn find_lambda_call_site<'a>(&self, lambda: &Node<'a>) -> Option<(Node<'a>, usize)> {
        let args_node = lambda.parent()?;
        if args_node.kind() != "argument_list" {
            return None;
        }
        let call_node = args_node.parent()?;
        if call_node.kind() != "method_invocation" {
            return None;
        }

        let mut arg_index = None;
        let mut idx = 0usize;
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if child == *lambda {
                arg_index = Some(idx);
                break;
            }
            idx += 1;
        }

        Some((call_node, arg_index?))
    }

    fn resolve_expected_argument_type(
        &self,
        call_node: &Node<'_>,
        lambda_node: &Node<'_>,
        arg_index: usize,
        ctx: &InferContext,
    ) -> Option<TypeRef> {
        let method_name_node = call_node.child_by_field_name("name")?;
        let method_name = method_name_node.utf8_text(ctx.source.as_bytes()).ok()?;

        let receiver_type = if let Some(receiver) = call_node.child_by_field_name("object") {
            super::infer_expression(&receiver, ctx)?
        } else {
            TypeRef::Id(ctx.enclosing_class.clone()?)
        };
        let receiver_fqn = receiver_type.as_fqn()?;

        let candidates = ctx.ts.find_member_in_hierarchy(&receiver_fqn, method_name);
        if candidates.is_empty() {
            // Heuristic fallback when external SDK methods are not indexed.
            // Example: List<A>.forEach(it -> it.hello()) => infer lambda param as A.
            if method_name == "forEach" {
                if let TypeRef::Generic { args, .. } = &receiver_type {
                    if let Some(first_arg) = args.first() {
                        return Some(TypeRef::Generic {
                            base: Box::new(TypeRef::Id(
                                "java.util.function.Consumer".to_string(),
                            )),
                            args: vec![first_arg.clone()],
                        });
                    }
                }
            }
            return None;
        }
        let candidates = self.apply_receiver_substitution(candidates, &receiver_type, ctx);

        let args_node = call_node.child_by_field_name("arguments")?;
        let mut arg_types = Vec::new();
        let mut cursor = args_node.walk();
        for child in args_node.children(&mut cursor) {
            if !child.is_named() {
                continue;
            }
            if child == *lambda_node {
                arg_types.push(TypeRef::Unknown);
            } else if let Some(t) = super::infer_expression(&child, ctx) {
                arg_types.push(t);
            } else {
                arg_types.push(TypeRef::Unknown);
            }
        }

        let resolved = ctx.ts.resolve_method(&candidates, &arg_types)?;
        let params = resolved.parameters?;
        params.get(arg_index).map(|p| p.type_ref.clone())
    }

    fn apply_receiver_substitution(
        &self,
        candidates: Vec<crate::inference::MemberInfo>,
        receiver_type: &TypeRef,
        ctx: &InferContext,
    ) -> Vec<crate::inference::MemberInfo> {
        let (base_fqn, receiver_args) = match receiver_type {
            TypeRef::Generic { base, args } => {
                let Some(base_fqn) = base.as_fqn() else {
                    return candidates;
                };
                (base_fqn, args)
            }
            _ => return candidates,
        };

        let Some(type_info) = ctx.ts.get_type_info(&base_fqn) else {
            return candidates;
        };
        if type_info.type_parameters.len() != receiver_args.len() {
            return candidates;
        }

        let mut subst = Substitution::new();
        for (param, arg) in type_info.type_parameters.iter().zip(receiver_args.iter()) {
            subst.insert(param.name.clone(), arg.clone());
        }

        candidates
            .into_iter()
            .map(|mut member| {
                member.type_ref = subst.apply(&member.type_ref);
                if let Some(params) = &mut member.parameters {
                    for p in params {
                        p.type_ref = subst.apply(&p.type_ref);
                    }
                }
                member
            })
            .collect()
    }

    fn extract_lambda_parameter_type(
        &self,
        expected_arg_type: &TypeRef,
        lambda_param_index: usize,
        ctx: &InferContext,
    ) -> Option<TypeRef> {
        if let TypeRef::Generic { base, args } = expected_arg_type {
            if lambda_param_index < args.len() {
                return Some(unwrap_wildcard(args[lambda_param_index].clone()));
            }

            if let Some(base_fqn) = base.as_fqn() {
                for method_name in ["accept", "apply", "test", "run", "call", "get"] {
                    let members = ctx.ts.find_member_in_hierarchy(&base_fqn, method_name);
                    for member in members {
                        if let Some(params) = member.parameters {
                            if let Some(param) = params.get(lambda_param_index) {
                                return Some(unwrap_wildcard(param.type_ref.clone()));
                            }
                        }
                    }
                }
            }
        }

        if let TypeRef::Wildcard { bound: Some(b), .. } = expected_arg_type {
            return Some((**b).clone());
        }

        None
    }
}

fn unwrap_wildcard(ty: TypeRef) -> TypeRef {
    match ty {
        TypeRef::Wildcard { bound: Some(b), .. } => *b,
        other => other,
    }
}

fn collect_lambda_parameter_names(params_node: &Node, source: &str) -> Vec<String> {
    if params_node.kind() == "identifier" {
        return params_node
            .utf8_text(source.as_bytes())
            .ok()
            .map(|s| vec![s.to_string()])
            .unwrap_or_default();
    }

    let mut names = Vec::new();
    let mut cursor = params_node.walk();
    for child in params_node.children(&mut cursor) {
        match child.kind() {
            "formal_parameter" | "spread_parameter" => {
                if let Some(name_node) = child.child_by_field_name("name") {
                    if let Ok(name) = name_node.utf8_text(source.as_bytes()) {
                        names.push(name.to_string());
                    }
                }
            }
            "inferred_parameters" => {
                names.extend(collect_lambda_parameter_names(&child, source));
            }
            "identifier" => {
                if let Ok(name) = child.utf8_text(source.as_bytes()) {
                    names.push(name.to_string());
                }
            }
            _ => {}
        }
    }
    names
}

/// Parse a type node into TypeRef.
pub fn parse_type_node(node: &Node, ctx: &InferContext) -> Option<TypeRef> {
    let kind = node.kind();

    match kind {
        // Primitive types
        "integral_type" | "floating_point_type" | "boolean_type" | "void_type" => {
            let text = node.utf8_text(ctx.source.as_bytes()).ok()?;
            Some(TypeRef::Raw(text.to_string()))
        }
        // Simple type identifier
        "type_identifier" => {
            let name = node.utf8_text(ctx.source.as_bytes()).ok()?;
            // Try to resolve to FQN
            let fqn = ctx
                .ts
                .resolve_type_name(name, &ctx.to_resolution_context())
                .unwrap_or_else(|| name.to_string());
            Some(TypeRef::Id(fqn))
        }
        // Scoped type like java.util.List
        "scoped_type_identifier" => {
            let text = node.utf8_text(ctx.source.as_bytes()).ok()?;
            Some(TypeRef::Id(text.replace(" ", "")))
        }
        // Generic type like List<String>
        "generic_type" => {
            let base_node = node.child_by_field_name("type").or_else(|| node.child(0))?;
            let base = parse_type_node(&base_node, ctx)?;

            let mut args = Vec::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "type_arguments" {
                    continue;
                }

                let mut args_cursor = child.walk();
                for arg in child.children(&mut args_cursor) {
                    if !arg.is_named() {
                        continue;
                    }
                    if let Some(parsed) = parse_type_node(&arg, ctx) {
                        args.push(parsed);
                    }
                }
            }

            Some(TypeRef::Generic {
                base: Box::new(base),
                args,
            })
        }
        // Array type
        "array_type" => {
            let element = node.child_by_field_name("element")?;
            let element_type = parse_type_node(&element, ctx)?;
            Some(TypeRef::Array {
                element: Box::new(element_type),
                dimensions: 1, // TODO: count dimensions properly
            })
        }
        _ => {
            // Unknown type node, try raw text
            node.utf8_text(ctx.source.as_bytes())
                .ok()
                .map(|s| TypeRef::Raw(s.to_string()))
        }
    }
}
