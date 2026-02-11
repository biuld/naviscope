//! Java resolver using the new inference-based type system.

pub mod context;
pub mod external;
pub mod lang;
pub mod semantic;
pub mod types;

use crate::inference::adapters::CodeGraphTypeSystem;
use crate::JavaPlugin;
use context::ResolutionContext;
use naviscope_api::models::{SymbolResolution, TypeRef};

impl JavaPlugin {
    /// Helper to find enclosing class using ScopeManager
    fn find_enclosing_class_via_scope(
        &self,
        node: tree_sitter::Node,
        scope_manager: &crate::inference::scope::ScopeManager,
    ) -> Option<String> {
        let mut current = node;
        while let Some(parent) = current.parent() {
            if let Some(sid) = scope_manager.get_scope_id(parent.id()) {
                return scope_manager.find_enclosing_class(sid);
            }
            current = parent;
        }
        None
    }

    /// Resolve a symbol using the new inference-based approach.
    pub fn resolve_symbol_internal(&self, context: &ResolutionContext) -> Option<SymbolResolution> {
        let ts = CodeGraphTypeSystem::new(context.index);

        // Extract package from tree
        let (package, imports) = self
            .parser
            .extract_package_and_imports(context.tree, context.source);

        // Build inference context
        // Initialize ScopeManager for efficient local variable lookup
        let mut scope_manager = crate::inference::scope::ScopeManager::new();

        // Create context with populated scopes
        // We scan the entire tree to build the scope table
        let mut infer_ctx = crate::inference::create_inference_context(
            &context.tree.root_node(),
            context.source,
            &ts,
            &mut scope_manager,
            package.clone(),
            imports.clone(),
        );

        // Add enclosing class if available from context, OR infer from AST via ScopeManager
        let enclosing_fqn = if let Some(class) = context.enclosing_classes.first() {
            Some(class.clone())
        } else {
            infer_ctx
                .scope_manager
                .and_then(|sm| self.find_enclosing_class_via_scope(context.node, sm))
        };

        if let Some(fqn) = enclosing_fqn {
            infer_ctx = infer_ctx.with_enclosing_class(fqn);
        }

        // 1. If this identifier is a declaration's name, resolve to that declaration's FQN
        if let Some(parent) = context.node.parent() {
            // Check if this node is the 'name' of a method_declaration or class_declaration
            if parent.child_by_field_name("name") == Some(context.node) {
                match parent.kind() {
                    "method_declaration" | "constructor_declaration" => {
                        // Build method FQN using canonical member separator
                        if let Some(ref enclosing) = infer_ctx.enclosing_class {
                            let method_fqn =
                                crate::naming::build_member_fqn(enclosing, &context.name);
                            return Some(SymbolResolution::Precise(
                                method_fqn,
                                naviscope_api::models::SymbolIntent::Method,
                            ));
                        }
                    }
                    "class_declaration" | "interface_declaration" | "enum_declaration" => {
                        // Build class FQN
                        let res_ctx = infer_ctx.to_resolution_context();
                        if let Some(fqn) = infer_ctx.ts.resolve_type_name(&context.name, &res_ctx) {
                            return Some(SymbolResolution::Precise(
                                fqn,
                                naviscope_api::models::SymbolIntent::Type,
                            ));
                        }
                    }
                    "variable_declarator" => {
                        // Check if this is a class field (parent's parent is field_declaration)
                        // or a local variable (parent's parent is local_variable_declaration)
                        if let Some(grandparent) = parent.parent() {
                            if grandparent.kind() == "field_declaration" {
                                // Build field FQN using canonical member separator
                                if let Some(ref enclosing) = infer_ctx.enclosing_class {
                                    let field_fqn =
                                        crate::naming::build_member_fqn(enclosing, &context.name);
                                    return Some(SymbolResolution::Precise(
                                        field_fqn,
                                        naviscope_api::models::SymbolIntent::Field,
                                    ));
                                }
                            }
                            // For local_variable_declaration, fall through to local variable handling
                        }
                    }
                    _ => {}
                }
            }
        }

        // 2. Handle 'this' specifically
        if context.node.kind() == "this" {
            if let Some(enclosing) = &infer_ctx.enclosing_class {
                return Some(SymbolResolution::Precise(
                    enclosing.clone(),
                    naviscope_api::models::SymbolIntent::Type,
                ));
            }
        }

        // 2.5. Check for local variable references (returns Local resolution)
        if context.node.kind() == "identifier" {
            if let Some(sm) = infer_ctx.scope_manager {
                // Walk up to find the nearest scope
                let mut current = context.node;
                let mut start_scope_id = None;
                while let Some(parent) = current.parent() {
                    if let Some(sid) = sm.get_scope_id(parent.id()) {
                        start_scope_id = Some(sid);
                        break;
                    }
                    current = parent;
                }

                if let Some(sid) = start_scope_id {
                    if let Some(info) = sm.lookup_symbol(sid, &context.name) {
                        // Ensure declaration is before usage
                        let usage_point = context.node.start_position();
                        let decl_line = info.range.start_line;
                        let decl_col = info.range.start_col;

                        // Compare position (row/line are 0-indexed in TS, assuming Range follows TS or is consistent)
                        if decl_line < usage_point.row
                            || (decl_line == usage_point.row && decl_col < usage_point.column)
                        {
                            // Extract type name from reference if possible, or stringify the type
                            // The old find_local_declaration returned Option<String> for type name.
                            // We have info.type_ref.
                            let type_name = match &info.type_ref {
                                TypeRef::Id(id) | TypeRef::Raw(id) => Some(id.clone()),
                                _ => None, // Complex types might need rendering
                            };
                            return Some(SymbolResolution::Local(info.range.clone(), type_name));
                        }
                    }
                }
            }
        }

        // 3. Resolve context-sensitive references (Methods, Fields)
        // If it's a method name identifier, resolve to the method FQN
        if let Some(parent) = context.node.parent() {
            if parent.kind() == "method_invocation"
                && parent.child_by_field_name("name") == Some(context.node)
            {
                if let Some(type_ref) =
                    crate::inference::strategy::MethodCallInfer.infer_member(&parent, &infer_ctx)
                {
                    return Some(SymbolResolution::Precise(type_ref, context.intent));
                }
            }
            if parent.kind() == "field_access"
                && parent.child_by_field_name("field") == Some(context.node)
            {
                if let Some(type_ref) =
                    crate::inference::strategy::FieldAccessInfer.infer_member(&parent, &infer_ctx)
                {
                    return Some(SymbolResolution::Precise(type_ref, context.intent));
                }
            }
        }

        // 4. Main inference path for everything else
        if let Some(type_ref) =
            crate::inference::strategy::infer_expression(&context.node, &infer_ctx)
        {
            if let TypeRef::Id(fqn) = &type_ref {
                return Some(SymbolResolution::Precise(fqn.clone(), context.intent));
            }
        }

        None
    }
}
