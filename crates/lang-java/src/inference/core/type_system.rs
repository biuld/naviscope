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
        let exact_fixed = collect_matching_candidates(candidates, |params| {
            matches_fixed_arity(params, arg_types, |arg, expected| arg == expected)
        });
        if !exact_fixed.is_empty() {
            return select_most_specific(self, exact_fixed, arg_types);
        }

        // 2. Subtype match (widening)
        let subtype_fixed = collect_matching_candidates(candidates, |params| {
            matches_fixed_arity(params, arg_types, |arg, expected| {
                self.is_subtype(arg, expected)
            })
        });
        if !subtype_fixed.is_empty() {
            return select_most_specific(self, subtype_fixed, arg_types);
        }

        // 3. Varargs exact match
        let exact_varargs = collect_matching_candidates(candidates, |params| {
            matches_varargs_arity(params, arg_types, |arg, expected| arg == expected)
        });
        if !exact_varargs.is_empty() {
            return select_most_specific(self, exact_varargs, arg_types);
        }

        // 4. Varargs subtype match
        let subtype_varargs = collect_matching_candidates(candidates, |params| {
            matches_varargs_arity(params, arg_types, |arg, expected| {
                self.is_subtype(arg, expected)
            })
        });
        if !subtype_varargs.is_empty() {
            return select_most_specific(self, subtype_varargs, arg_types);
        }

        // Strict mode: if no overload matches exactly or via subtype,
        // we return None. No fallback to arbitrary candidate.
        None
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

fn collect_matching_candidates<F>(candidates: &[MemberInfo], mut matches: F) -> Vec<MemberInfo>
where
    F: FnMut(&[ParameterInfo]) -> bool,
{
    candidates
        .iter()
        .filter_map(|cand| {
            let params = cand.parameters.as_ref()?;
            if matches(params) {
                Some(cand.clone())
            } else {
                None
            }
        })
        .collect()
}

fn select_most_specific<T: JavaTypeSystem + ?Sized>(
    ts: &T,
    candidates: Vec<MemberInfo>,
    arg_types: &[TypeRef],
) -> Option<MemberInfo> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return candidates.into_iter().next();
    }

    let mut best_idx = 0usize;
    let mut best_score = i32::MIN;

    for (i, cand) in candidates.iter().enumerate() {
        let mut score = 0i32;
        for (j, other) in candidates.iter().enumerate() {
            if i == j {
                continue;
            }
            let cand_more_specific = is_more_specific_than(ts, cand, other, arg_types);
            let other_more_specific = is_more_specific_than(ts, other, cand, arg_types);
            if cand_more_specific && !other_more_specific {
                score += 1;
            } else if other_more_specific && !cand_more_specific {
                score -= 1;
            }
        }

        if score > best_score {
            best_score = score;
            best_idx = i;
        }
    }

    candidates.get(best_idx).cloned()
}

fn is_more_specific_than<T: JavaTypeSystem + ?Sized>(
    ts: &T,
    left: &MemberInfo,
    right: &MemberInfo,
    arg_types: &[TypeRef],
) -> bool {
    let Some(left_types) = effective_param_types(left, arg_types.len()) else {
        return false;
    };
    let Some(right_types) = effective_param_types(right, arg_types.len()) else {
        return false;
    };
    if left_types.len() != right_types.len() {
        return false;
    }

    let mut strict = false;
    for (l, r) in left_types.iter().zip(right_types.iter()) {
        if l == r {
            continue;
        }
        if ts.is_subtype(l, r) {
            strict = true;
        } else {
            return false;
        }
    }
    strict
}

fn effective_param_types(member: &MemberInfo, arg_count: usize) -> Option<Vec<TypeRef>> {
    let params = member.parameters.as_ref()?;
    let Some(last) = params.last() else {
        return Some(vec![]);
    };

    if !is_varargs_parameter(last) {
        if params.len() == arg_count {
            return Some(params.iter().map(|p| p.type_ref.clone()).collect());
        }
        return None;
    }

    let TypeRef::Array { element, .. } = &last.type_ref else {
        return None;
    };
    let fixed_count = params.len() - 1;
    if arg_count < fixed_count {
        return None;
    }

    let mut types = Vec::with_capacity(arg_count);
    for p in &params[..fixed_count] {
        types.push(p.type_ref.clone());
    }
    for _ in fixed_count..arg_count {
        types.push(element.as_ref().clone());
    }
    Some(types)
}
