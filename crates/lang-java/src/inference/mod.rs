//! Java Type Inference System
//!
//! This module provides a functional-style type inference engine for Java.
//!
//! # Architecture
//!
//! ```text
//! InferStrategy (trait)    →  combines via or_else(), map()
//!       │
//!       ▼
//! InferContext (immutable) →  passed through inference chain
//!       │
//!       ▼
//! JavaTypeSystem (trait)   →  provides type/member lookup
//! ```
//!
//! # Key Traits
//!
//! - [`TypeProvider`] - Resolves FQN to type information
//! - [`InheritanceProvider`] - Walks supertype hierarchy
//! - [`MemberProvider`] - Finds members in types
//! - [`JavaTypeSystem`] - Combines all three
//!
//! # Usage
//!
//! ```ignore
//! use naviscope_java::inference::{InferContext, infer_expression};
//!
//! let ctx = InferContext::new(source, &type_system);
//! let result = infer_expression(&node, &ctx);
//! ```

pub mod adapters;
mod chain;
pub mod context;
pub mod core;
pub mod scope;
pub mod strategy;

// Re-export public API
pub use core::type_system::{InheritanceProvider, JavaTypeSystem, MemberProvider, TypeProvider};

pub use core::types::{
    MemberInfo, MemberKind, ParameterInfo, TypeInfo, TypeKind, TypeRefExt, TypeResolutionContext,
};

pub use context::InferContext;

pub use chain::{ChainResolution, resolve_chain};
pub use strategy::{InferStrategy, infer_expression};

use crate::inference::scope::{ScopeBuilder, ScopeManager};

/// Creates a new `InferContext` that is fully populated with scope information.
///
/// This helper:
/// 1. Creates a `ScopeManager`.
/// 2. Iterates over the entire source file (AST root) to build scopes.
/// 3. Returns an `InferContext` ready for type inference.
pub fn create_inference_context<'a>(
    root: &tree_sitter::Node,
    source: &'a str,
    ts: &'a dyn JavaTypeSystem,
    scope_manager: &'a mut ScopeManager,
    package: Option<String>,
    imports: Vec<String>,
) -> InferContext<'a> {
    let mut ctx = InferContext::new(source, ts)
        .with_package(package)
        .with_imports(imports);

    // 1. Pre-scan scopes
    {
        let mut builder = ScopeBuilder::new(&ctx, scope_manager);
        builder.build(root);
    } // builder dropped, releasing mutable borrow

    // 2. Extract known FQNs
    let known_fqns = scope_manager.get_all_class_fqns();
    ctx.known_fqns = known_fqns;

    // 3. Attach populated scope manager to context
    ctx.scope_manager = Some(scope_manager); // Downgrade to shared reference

    ctx
}
