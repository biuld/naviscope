//! Subtyping rules implementation.
//!
//! Determines if one type is a subtype of another.
//! Supports:
//! - Identity
//! - Primitives (widening)
//! - Classes (extends)
//! - Interfaces (implements)
//! - Arrays (covariant)

use crate::inference::core::type_system::JavaTypeSystem;
use naviscope_api::models::TypeRef;

/// Check if `sub` is a subtype of `super_type`.
pub fn is_subtype<T: JavaTypeSystem + ?Sized>(sub: &TypeRef, super_type: &TypeRef, ts: &T) -> bool {
    // 1. Reflexivity
    if sub == super_type {
        return true;
    }

    // 2. java.lang.Object is supertype of all reference types
    if let TypeRef::Id(id) = super_type {
        if id == "java.lang.Object" {
            // Primitives are not Objects (unless autoboxed, but strict subtyping usually separates them)
            // For now, assume strict subtyping for checking
            return !matches!(sub, TypeRef::Raw(_));
        }
    }

    match (sub, super_type) {
        // Primitive widening
        (TypeRef::Raw(s1), TypeRef::Raw(s2)) => is_primitive_subtype(s1, s2),

        // Primitive boxing + reference widening, e.g. int -> Integer / Number / Object
        (TypeRef::Raw(prim), TypeRef::Id(super_id)) => {
            if let Some(wrapper) = primitive_to_wrapper(prim) {
                if wrapper == super_id {
                    return true;
                }
                return is_class_subtype(wrapper, super_id, ts);
            }
            false
        }

        // Wrapper unboxing (+ primitive widening), e.g. Integer -> int / long
        (TypeRef::Id(sub_id), TypeRef::Raw(super_prim)) => {
            if let Some(unboxed) = wrapper_to_primitive(sub_id) {
                return unboxed == super_prim || is_primitive_subtype(unboxed, super_prim);
            }
            false
        }

        // Class/Interface hierarchy
        (TypeRef::Id(sub_id), TypeRef::Id(super_id)) => is_class_subtype(sub_id, super_id, ts),

        // Arrays (Covariant for references)
        (TypeRef::Array { element: e1, .. }, TypeRef::Array { element: e2, .. }) => {
            is_subtype(e1, e2, ts)
        }

        // TODO: Generics (Invariant? Covariant with wildcards?)
        // For now, simple equality on generics was caught by step 1
        _ => false,
    }
}

fn primitive_to_wrapper(primitive: &str) -> Option<&'static str> {
    match primitive {
        "byte" => Some("java.lang.Byte"),
        "short" => Some("java.lang.Short"),
        "char" => Some("java.lang.Character"),
        "int" => Some("java.lang.Integer"),
        "long" => Some("java.lang.Long"),
        "float" => Some("java.lang.Float"),
        "double" => Some("java.lang.Double"),
        "boolean" => Some("java.lang.Boolean"),
        _ => None,
    }
}

fn wrapper_to_primitive(wrapper: &str) -> Option<&'static str> {
    match wrapper {
        "java.lang.Byte" => Some("byte"),
        "java.lang.Short" => Some("short"),
        "java.lang.Character" => Some("char"),
        "java.lang.Integer" => Some("int"),
        "java.lang.Long" => Some("long"),
        "java.lang.Float" => Some("float"),
        "java.lang.Double" => Some("double"),
        "java.lang.Boolean" => Some("boolean"),
        _ => None,
    }
}

fn is_primitive_subtype(sub: &str, sup: &str) -> bool {
    match sub {
        "byte" => matches!(sup, "short" | "int" | "long" | "float" | "double"),
        "short" => matches!(sup, "int" | "long" | "float" | "double"),
        "char" => matches!(sup, "int" | "long" | "float" | "double"),
        "int" => matches!(sup, "long" | "float" | "double"),
        "long" => matches!(sup, "float" | "double"),
        "float" => matches!(sup, "double"),
        _ => false,
    }
}

fn is_class_subtype<T: JavaTypeSystem + ?Sized>(sub_fqn: &str, super_fqn: &str, ts: &T) -> bool {
    if sub_fqn == super_fqn {
        return true;
    }

    // BFS search up the hierarchy
    // JavaTypeSystem::walk_ancestors returns an iterator of all ancestors
    for ancestor in ts.walk_ancestors(sub_fqn) {
        if ancestor == super_fqn {
            return true;
        }
    }

    false
}
