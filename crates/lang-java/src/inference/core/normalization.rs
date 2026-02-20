//! Type normalization logic.
//!
//! Converts raw/string types into structured, fully-qualified TypeRefs.

use crate::inference::InferContext;
use naviscope_api::models::TypeRef;

/// Normalize a TypeRef using the given context.
///
/// This resolves simple names to FQNs and handles generics.
pub fn normalize_type(ty: TypeRef, ctx: &InferContext) -> TypeRef {
    match ty {
        TypeRef::Raw(s) => {
            // Try to parse and resolve
            // For now, simple FQN resolution
            if let Some(fqn) = ctx.ts.resolve_type_name(&s, &ctx.to_resolution_context()) {
                TypeRef::Id(fqn)
            } else {
                // If not resolved, keep as Raw or assume it's FQN if contains dot?
                if s.contains('.') {
                    TypeRef::Id(s)
                } else {
                    TypeRef::Raw(s)
                }
            }
        }
        TypeRef::Id(id) => {
            // Ensure ID is fully qualified if possible
            if !id.contains('.') {
                if let Some(fqn) = ctx.ts.resolve_type_name(&id, &ctx.to_resolution_context()) {
                    TypeRef::Id(fqn)
                } else {
                    TypeRef::Id(id)
                }
            } else {
                TypeRef::Id(id)
            }
        }
        TypeRef::Generic { base, args } => {
            let base = Box::new(normalize_type(*base, ctx));
            let args = args
                .into_iter()
                .map(|arg| normalize_type(arg, ctx))
                .collect();
            TypeRef::Generic { base, args }
        }
        TypeRef::Array {
            element,
            dimensions,
        } => TypeRef::Array {
            element: Box::new(normalize_type(*element, ctx)),
            dimensions,
        },
        _ => ty,
    }
}
