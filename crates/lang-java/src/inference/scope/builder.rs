//! Scope Builder implementation.
//!
//! Walks the AST to populate the ScopeManager with variable declarations.

use super::ScopeManager;
use super::manager::ScopeKind;
use crate::inference::InferContext;
use naviscope_api::models::TypeRef;
use naviscope_plugin::utils::range_from_ts;
use tree_sitter::{Node, TreeCursor};

/// Builds the scope tree and symbol table for a method or block.
pub struct ScopeBuilder<'a, 'b> {
    ctx: &'a InferContext<'b>,
    manager: &'a mut ScopeManager,
}

impl<'a, 'b> ScopeBuilder<'a, 'b> {
    pub fn new(ctx: &'a InferContext<'b>, manager: &'a mut ScopeManager) -> Self {
        Self { ctx, manager }
    }

    /// Build scopes for the given root node (e.g., method_declaration)
    pub fn build(&mut self, root: &Node) {
        let mut cursor = root.walk();
        self.visit_node(root, &mut cursor, None, None);
    }

    fn visit_node(
        &mut self,
        node: &Node,
        cursor: &mut TreeCursor,
        parent_scope: Option<usize>,
        fqn_prefix: Option<String>,
    ) {
        let mut current_scope = parent_scope;
        let mut next_fqn_prefix = fqn_prefix.clone();

        // Check if this node creates a new scope
        if is_scope_creator(node.kind()) {
            let kind = self.determine_scope_kind(node, &fqn_prefix);

            // If it is a class scope, update the prefix for children
            if let ScopeKind::Class(ref fqn) = kind {
                next_fqn_prefix = Some(fqn.clone());
            }

            // Register new scope
            // Use node.id() as the scope ID/key
            let scope_id = self.manager.register_scope(node.id(), parent_scope, kind);
            current_scope = Some(scope_id);

            // Register parameters if this is a method or lambda or catch
            self.register_parameters(node, scope_id);
        }

        // Check if this node declares a variable in the current scope
        if let Some(scope_id) = current_scope {
            self.register_variable_declarations(node, scope_id);
        }

        // Recurse children
        if node.child_count() > 0 {
            if cursor.goto_first_child() {
                loop {
                    let child = cursor.node();
                    self.visit_node(&child, cursor, current_scope, next_fqn_prefix.clone());
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
        }
    }

    fn determine_scope_kind(&self, node: &Node, prefix: &Option<String>) -> ScopeKind {
        match node.kind() {
            "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "annotation_type_declaration" => {
                let name = node
                    .child_by_field_name("name")
                    .and_then(|n| n.utf8_text(self.ctx.source.as_bytes()).ok())
                    .unwrap_or("Anonymous");

                let fqn = if let Some(p) = prefix {
                    format!("{}.{}", p, name)
                } else if let Some(pkg) = &self.ctx.package {
                    format!("{}.{}", pkg, name)
                } else {
                    name.to_string()
                };

                ScopeKind::Class(fqn)
            }
            "method_declaration" | "constructor_declaration" => ScopeKind::Method,
            _ => ScopeKind::Local,
        }
    }

    fn register_parameters(&mut self, node: &Node, scope_id: usize) {
        match node.kind() {
            "method_declaration" | "constructor_declaration" => {
                if let Some(params) = node.child_by_field_name("parameters") {
                    self.process_parameter_list(&params, scope_id);
                }
            }
            "lambda_expression" => {
                if let Some(params) = node.child_by_field_name("parameters") {
                    // Start simple: explicit typed params
                    // Inferred params need type inference (circular dependency if we use InferStrategy here?)
                    // For now, we only register fully typed params or basic names
                    self.process_parameter_list(&params, scope_id);
                }
            }
            "catch_clause" => {
                if let Some(param) = node.child_by_field_name("parameter") {
                    self.process_single_parameter(&param, scope_id);
                }
            }
            "enhanced_for_statement" => {
                // for (Type name : iterable)
                if let (Some(ty_node), Some(name_node)) = (
                    node.child_by_field_name("type"),
                    node.child_by_field_name("name"),
                ) {
                    if let Some(ty) = self.parse_type(&ty_node) {
                        if let Ok(name) = name_node.utf8_text(self.ctx.source.as_bytes()) {
                            let range = range_from_ts(name_node.range());
                            self.manager
                                .add_symbol(scope_id, name.to_string(), ty, range);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    fn process_parameter_list(&mut self, params_node: &Node, scope_id: usize) {
        let mut cursor = params_node.walk();
        for child in params_node.children(&mut cursor) {
            match child.kind() {
                "formal_parameter" | "spread_parameter" => {
                    self.process_single_parameter(&child, scope_id);
                }
                "inferred_parameters" => {
                    // TODO: Inferred lambda parameters require target type context
                }
                "identifier" => {
                    // Single lambda param: x -> ...
                    if let Ok(name) = child.utf8_text(self.ctx.source.as_bytes()) {
                        // Type is unknown at this stage without context
                        // Register with Unknown type - inference can refine later
                        let range = range_from_ts(child.range());
                        self.manager.add_symbol(
                            scope_id,
                            name.to_string(),
                            TypeRef::Unknown,
                            range,
                        );
                    }
                }
                _ => {}
            }
        }
    }

    fn process_single_parameter(&mut self, param: &Node, scope_id: usize) {
        let name_node = param.child_by_field_name("name");
        let type_node = param.child_by_field_name("type");

        if let (Some(name_node), Some(type_node)) = (name_node, type_node) {
            if let Ok(name) = name_node.utf8_text(self.ctx.source.as_bytes()) {
                if let Some(ty) = self.parse_type(&type_node) {
                    let range = range_from_ts(name_node.range());
                    self.manager
                        .add_symbol(scope_id, name.to_string(), ty, range);
                }
            }
        }
    }

    fn register_variable_declarations(&mut self, node: &Node, scope_id: usize) {
        if node.kind() == "local_variable_declaration" {
            // Type
            let mut ty: Option<TypeRef> = None;

            // First child that is not modifier is usually type
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() != "modifiers" && child.kind() != "variable_declarator" {
                    ty = self.parse_type(&child);
                    break;
                }
            }

            if let Some(valid_type) = ty {
                // Declarators
                for child in node.children(&mut cursor) {
                    if child.kind() == "variable_declarator" {
                        if let Some(name_node) = child.child_by_field_name("name") {
                            if let Ok(name) = name_node.utf8_text(self.ctx.source.as_bytes()) {
                                let range = range_from_ts(name_node.range());
                                self.manager.add_symbol(
                                    scope_id,
                                    name.to_string(),
                                    valid_type.clone(),
                                    range,
                                );
                            }
                        }
                    }
                }
            } else if let Some(_var_node) = node.children(&mut cursor).find(|c| {
                c.kind() == "var" || c.utf8_text(self.ctx.source.as_bytes()).unwrap_or("") == "var"
            }) {
                // Handle 'var' inference
                // This is tricky: we are building scope to help inference, but 'var' needs inference.
                // We can try to infer the initializer.
                // let mut cursor = node.walk(); // Cannot reuse cursor because it's borrowed by children iterator?
                // Actually `node.children(&mut cursor)` borrows cursor.
                // We need to finish the previous iteration or use a new cursor.
                // Since `node.children` consumes the borrow for the loop, we can't reuse it easily if we broke out?
                // But here we are reusing `cursor` from line 141.

                // Let's just create a new cursor for the var inference part to avoid complexity
                let mut var_cursor = node.walk();
                for child in node.children(&mut var_cursor) {
                    if child.kind() == "variable_declarator" {
                        if let (Some(_name_node), Some(_value_node)) = (
                            child.child_by_field_name("name"),
                            child.child_by_field_name("value"),
                        ) {
                            // TODO: Recursively call inference on value?
                        }
                    }
                }
            }
        }
    }

    fn parse_type(&self, node: &Node) -> Option<TypeRef> {
        use crate::inference::core::normalization::normalize_type;
        use crate::inference::strategy::local::parse_type_node;

        let raw_type = parse_type_node(node, self.ctx)?;
        Some(normalize_type(raw_type, self.ctx))
    }
}

fn is_scope_creator(kind: &str) -> bool {
    matches!(
        kind,
        "class_declaration"
            | "interface_declaration"
            | "enum_declaration"
            | "annotation_type_declaration"
            | "method_declaration"
            | "constructor_declaration"
            | "block"
            | "lambda_expression"
            | "catch_clause"
            | "enhanced_for_statement"
            | "for_statement"
            | "try_with_resources_statement"
    )
}
