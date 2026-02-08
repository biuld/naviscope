//! Type unification and substitution.
//!
//! Handles generic type parameter resolution and constraints solving.

use naviscope_api::models::TypeRef;
use std::collections::HashMap;

/// A map from type variables to concrete types.
#[derive(Debug, Default, Clone)]
pub struct Substitution {
    map: HashMap<String, TypeRef>,
}

impl Substitution {
    /// Create a new empty substitution.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a mapping.
    pub fn insert(&mut self, var: String, ty: TypeRef) {
        self.map.insert(var, ty);
    }

    /// Apply this substitution to a type.
    pub fn apply(&self, ty: &TypeRef) -> TypeRef {
        match ty {
            TypeRef::Id(name) => {
                if let Some(sub) = self.map.get(name) {
                    sub.clone()
                } else {
                    ty.clone()
                }
            }
            TypeRef::Array {
                element,
                dimensions,
            } => TypeRef::Array {
                element: Box::new(self.apply(element)),
                dimensions: *dimensions,
            },
            TypeRef::Generic { base, args } => TypeRef::Generic {
                base: Box::new(self.apply(base)),
                args: args.iter().map(|arg| self.apply(arg)).collect(),
            },
            // Primitives and others remain unchanged
            _ => ty.clone(),
        }
    }
}

/// Attempt to unify two types, producing a substitution that makes them equal.
///
/// Note: Very basic implementation. Does not handle deep recursive constraints well yet.
pub fn unify(t1: &TypeRef, t2: &TypeRef) -> Option<Substitution> {
    let mut subst = Substitution::new();
    if unify_internal(t1, t2, &mut subst) {
        Some(subst)
    } else {
        None
    }
}

fn unify_internal(t1: &TypeRef, t2: &TypeRef, subst: &mut Substitution) -> bool {
    // 1. Resolve current substitutions
    let t1_resolved = subst.apply(t1);
    let t2_resolved = subst.apply(t2);

    if t1_resolved == t2_resolved {
        return true;
    }

    match (t1_resolved, t2_resolved) {
        // Variable binding (very simplified: treating any single ID as potential var if not FQN)
        // In reality, we need to know which IDs are "type variables" vs "concrete types".
        // For now, assume single-letter IDs might be variables (heuristic).
        (TypeRef::Id(v), other) if is_type_variable(&v) => {
            // Occurs check omitted for simplicity
            subst.insert(v, other);
            true
        }
        (other, TypeRef::Id(v)) if is_type_variable(&v) => {
            subst.insert(v, other);
            true
        }

        // Structural unification
        (TypeRef::Array { element: e1, .. }, TypeRef::Array { element: e2, .. }) => {
            unify_internal(&e1, &e2, subst)
        }
        (TypeRef::Generic { base: b1, args: a1 }, TypeRef::Generic { base: b2, args: a2 }) => {
            if !unify_internal(&b1, &b2, subst) {
                return false;
            }
            if a1.len() != a2.len() {
                return false;
            }
            for (arg1, arg2) in a1.iter().zip(a2.iter()) {
                if !unify_internal(arg1, arg2, subst) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

fn is_type_variable(s: &str) -> bool {
    // Heuristic: Single uppercase letter is likely a type var (T, E, K, V)
    s.len() == 1 && s.chars().next().unwrap().is_uppercase()
}
