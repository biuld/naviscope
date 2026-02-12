//! Core trait definitions for the type system abstraction.
//!
//! These traits abstract away the data source, allowing the inference
//! engine to work with CodeGraph, stubs, or mock implementations.

use super::types::{MemberInfo, ParameterInfo, TypeInfo, TypeResolutionContext};

/// Provides type information by FQN.
///
/// This is the primary way to look up type metadata.
pub trait TypeProvider: Send + Sync {
    /// Get type info for a fully qualified name.
    ///
    /// Returns `None` if the type is not found.
    fn get_type_info(&self, fqn: &str) -> Option<TypeInfo>;

    /// Resolve a simple type name to its FQN.
    ///
    /// Uses the provided context (imports, package) to resolve the name.
    fn resolve_type_name(
        &self,
        simple_name: &str,
        context: &TypeResolutionContext,
    ) -> Option<String>;
}

/// Provides inheritance relationship information.
///
/// This is used to traverse the type hierarchy when looking for inherited members.
pub trait InheritanceProvider: Send + Sync {
    /// Get the direct superclass of a type.
    ///
    /// Returns `None` for `java.lang.Object` or interfaces.
    fn get_superclass(&self, fqn: &str) -> Option<String>;

    /// Get the interfaces directly implemented by a type.
    fn get_interfaces(&self, fqn: &str) -> Vec<String>;

    /// Walk all ancestor types (superclasses and interfaces).
    ///
    /// The iterator yields types in BFS order, stopping at max depth.
    fn walk_ancestors(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_>;

    /// Get all direct subtypes (classes that extend or implement this type).
    fn get_direct_subtypes(&self, fqn: &str) -> Vec<String>;

    /// Walk all descendant types (subclasses and sub-interfaces).
    fn walk_descendants(&self, fqn: &str) -> Box<dyn Iterator<Item = String> + '_>;
}

/// Provides member (field/method) lookup.
///
/// This is used to find members within a single type (not walking inheritance).
pub trait MemberProvider: Send + Sync {
    /// Find all members directly declared in the given type with the matching name.
    ///
    /// Does NOT search the inheritance hierarchy.
    fn get_members(&self, type_fqn: &str, member_name: &str) -> Vec<MemberInfo>;

    /// Get all members directly declared in the given type.
    fn get_all_members(&self, type_fqn: &str) -> Vec<MemberInfo>;
}

use naviscope_api::models::TypeRef;

/// The combined type system interface.
///
/// Provides a unified facade for type inference operations.
/// Includes a default implementation for hierarchy search.
pub trait JavaTypeSystem: TypeProvider + InheritanceProvider + MemberProvider {
    /// Find a member in the type hierarchy.
    ///
    /// Searches the type itself first, then walks ancestors.
    fn find_member_in_hierarchy(&self, type_fqn: &str, member_name: &str) -> Vec<MemberInfo> {
        // Check the type itself first
        let mut members = self.get_members(type_fqn, member_name);
        if !members.is_empty() {
            return members;
        }

        // Walk ancestors
        for ancestor in self.walk_ancestors(type_fqn) {
            members = self.get_members(&ancestor, member_name);
            if !members.is_empty() {
                return members;
            }
        }

        vec![]
    }

    /// Resolve the best matching method among candidates based on argument types.
    ///
    /// Implements basic Java overload resolution rules.
    fn resolve_method(
        &self,
        candidates: &[MemberInfo],
        arg_types: &[TypeRef],
    ) -> Option<MemberInfo> {
        if candidates.is_empty() {
            return None;
        }

        // 1. Precise match (exact signatures)
        for cand in candidates {
            if let Some(params) = &cand.parameters {
                if params.len() == arg_types.len() {
                    let mut match_all = true;
                    for (p, a) in params.iter().zip(arg_types.iter()) {
                        if p.type_ref != *a {
                            match_all = false;
                            break;
                        }
                    }
                    if match_all {
                        return Some(cand.clone());
                    }
                }
            }
        }

        // 2. Subtype match (widening)
        for cand in candidates {
            if let Some(params) = &cand.parameters {
                if matches_fixed_arity(params, arg_types, |arg, expected| {
                    self.is_subtype(arg, expected)
                }) {
                    return Some(cand.clone());
                }
            }
        }

        // 3. Varargs exact match (heuristic: last array parameter is treated as varargs)
        for cand in candidates {
            if let Some(params) = &cand.parameters {
                if matches_varargs_arity(params, arg_types, |arg, expected| arg == expected) {
                    return Some(cand.clone());
                }
            }
        }

        // 4. Varargs subtype match
        for cand in candidates {
            if let Some(params) = &cand.parameters {
                if matches_varargs_arity(params, arg_types, |arg, expected| {
                    self.is_subtype(arg, expected)
                }) {
                    return Some(cand.clone());
                }
            }
        }

        // Fallback to first if no parameters expected or provided
        // (Helpful for field access or methods we couldn't match precisely)
        candidates.first().cloned()
    }

    /// Check if sub is a subtype of super_type.
    ///
    /// This is a basic implementation that can be overridden by more sophisticated logic.
    /// Check if sub is a subtype of super_type.
    ///
    /// Delegates to `subtyping::is_subtype` logic.
    fn is_subtype(&self, sub: &TypeRef, super_type: &TypeRef) -> bool {
        crate::inference::core::subtyping::is_subtype(sub, super_type, self)
    }
}

// Blanket implementation: any type implementing all three traits gets JavaTypeSystem
impl<T: TypeProvider + InheritanceProvider + MemberProvider> JavaTypeSystem for T {}

fn matches_fixed_arity<F>(params: &[ParameterInfo], arg_types: &[TypeRef], mut matches: F) -> bool
where
    F: FnMut(&TypeRef, &TypeRef) -> bool,
{
    if params.len() != arg_types.len() {
        return false;
    }

    params
        .iter()
        .zip(arg_types.iter())
        .all(|(p, a)| matches(a, &p.type_ref))
}

fn matches_varargs_arity<F>(params: &[ParameterInfo], arg_types: &[TypeRef], mut matches: F) -> bool
where
    F: FnMut(&TypeRef, &TypeRef) -> bool,
{
    let Some(last_param) = params.last() else {
        return false;
    };

    if !is_varargs_parameter(last_param) {
        return false;
    }

    let TypeRef::Array { element, .. } = &last_param.type_ref else {
        return false;
    };

    let fixed_count = params.len() - 1;
    if arg_types.len() < fixed_count {
        return false;
    }

    // Prefix arguments (before varargs tail)
    if !params[..fixed_count]
        .iter()
        .zip(arg_types[..fixed_count].iter())
        .all(|(p, a)| matches(a, &p.type_ref))
    {
        return false;
    }

    // No varargs arguments provided
    if arg_types.len() == fixed_count {
        return true;
    }

    // Direct array pass-through: foo(String[]) called with one String[] argument.
    if arg_types.len() == params.len() && matches(&arg_types[fixed_count], &last_param.type_ref) {
        return true;
    }

    // Expanded varargs: foo(String...) called with N String arguments.
    arg_types[fixed_count..]
        .iter()
        .all(|a| matches(a, element.as_ref()))
}

fn is_varargs_parameter(param: &ParameterInfo) -> bool {
    param.is_varargs
}
