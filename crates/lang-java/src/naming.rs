// Re-export cross-language naming utilities from plugin
pub use naviscope_plugin::naming::{
    MEMBER_SEPARATOR, MethodSignature, TYPE_SEPARATOR, build_member_fqn, extract_member_name,
    extract_simple_name, extract_type_fqn, format_method_name, has_method_signature, is_member_fqn,
    parse_member_fqn, parse_method_signature,
};

/// Java uses standard dot-separated paths for types and packages,
/// and standard hash-separated paths for members in our graph.
/// Thus we can alias directly to StandardNamingConvention.
pub use naviscope_plugin::StandardNamingConvention as JavaNamingConvention;

// ---------------------------------------------------------------------------
// Java-specific type normalization for method signatures
//
// Java uses type erasure for generics at the bytecode level, so the canonical
// method signature erases generic type arguments. These rules are Java/JVM
// specific and do NOT belong in the cross-language plugin crate.
// ---------------------------------------------------------------------------

use naviscope_api::models::TypeRef;

/// Normalize a `TypeRef` into its canonical signature string using Java rules.
///
/// Java-specific rules:
/// - Primitives / raw unresolved types: as-is (`int`, `boolean`, `void`)
/// - Resolved reference types: full FQN (`java.lang.String`)
/// - Arrays: element type + `[]` per dimension (`java.lang.String[]`)
/// - Generics: **erased** to base type (`java.util.List<String>` → `java.util.List`)
/// - Varargs: caller should convert to array **before** calling this function
///   (use [`varargs_to_array_type`])
/// - Wildcards / Unknown: `?`
pub fn normalize_type_for_signature(type_ref: &TypeRef) -> String {
    match type_ref {
        TypeRef::Raw(s) => s.clone(),
        TypeRef::Id(fqn) => fqn.clone(),
        TypeRef::Array {
            element,
            dimensions,
        } => {
            let base = normalize_type_for_signature(element);
            let brackets: String = "[]".repeat(*dimensions);
            format!("{}{}", base, brackets)
        }
        TypeRef::Generic { base, .. } => {
            // Java type erasure: discard generic type arguments
            normalize_type_for_signature(base)
        }
        TypeRef::Wildcard { .. } => "?".to_string(),
        TypeRef::Unknown => "?".to_string(),
    }
}

/// Convert a varargs parameter type to its array equivalent for signature purposes.
///
/// Java varargs `String...` should be represented as `String[]` in the method signature.
/// Call this before passing the type to signature construction.
///
/// If the type is already an array, an additional dimension is **not** added —
/// the parser is assumed to have already handled the varargs→array conversion.
pub fn varargs_to_array_type(type_ref: &TypeRef) -> TypeRef {
    match type_ref {
        TypeRef::Array { .. } => type_ref.clone(),
        other => TypeRef::Array {
            element: Box::new(other.clone()),
            dimensions: 1,
        },
    }
}

/// Build a Java method member name with its parameter signature.
///
/// This combines Java-specific type normalization with the cross-language
/// `format_method_name`. The result can be passed to `build_member_fqn`.
///
/// # Examples
///
/// ```ignore
/// let signed = build_java_method_name("target", &[TypeRef::raw("int")]);
/// // signed == "target(int)"
/// let fqn = build_member_fqn("com.example.A", &signed);
/// // fqn == "com.example.A#target(int)"
/// ```
pub fn build_java_method_name(name: &str, param_types: &[TypeRef]) -> String {
    let normalized: Vec<String> = param_types
        .iter()
        .map(normalize_type_for_signature)
        .collect();
    let refs: Vec<&str> = normalized.iter().map(|s| s.as_str()).collect();
    format_method_name(name, &refs)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- normalize_type_for_signature --

    #[test]
    fn normalize_primitive() {
        assert_eq!(normalize_type_for_signature(&TypeRef::raw("int")), "int");
        assert_eq!(
            normalize_type_for_signature(&TypeRef::raw("boolean")),
            "boolean"
        );
        assert_eq!(normalize_type_for_signature(&TypeRef::raw("void")), "void");
    }

    #[test]
    fn normalize_resolved_reference() {
        assert_eq!(
            normalize_type_for_signature(&TypeRef::id("java.lang.String")),
            "java.lang.String"
        );
    }

    #[test]
    fn normalize_array_single_dimension() {
        let arr = TypeRef::Array {
            element: Box::new(TypeRef::id("java.lang.String")),
            dimensions: 1,
        };
        assert_eq!(normalize_type_for_signature(&arr), "java.lang.String[]");
    }

    #[test]
    fn normalize_array_multi_dimension() {
        let arr = TypeRef::Array {
            element: Box::new(TypeRef::raw("int")),
            dimensions: 2,
        };
        assert_eq!(normalize_type_for_signature(&arr), "int[][]");
    }

    #[test]
    fn normalize_generic_erases_type_args() {
        let generic = TypeRef::Generic {
            base: Box::new(TypeRef::id("java.util.List")),
            args: vec![TypeRef::id("java.lang.String")],
        };
        assert_eq!(normalize_type_for_signature(&generic), "java.util.List");
    }

    #[test]
    fn normalize_nested_generic_erases_all() {
        let generic = TypeRef::Generic {
            base: Box::new(TypeRef::id("java.util.Map")),
            args: vec![
                TypeRef::id("java.lang.String"),
                TypeRef::Generic {
                    base: Box::new(TypeRef::id("java.util.List")),
                    args: vec![TypeRef::id("java.lang.Integer")],
                },
            ],
        };
        assert_eq!(normalize_type_for_signature(&generic), "java.util.Map");
    }

    #[test]
    fn normalize_generic_array() {
        let arr = TypeRef::Array {
            element: Box::new(TypeRef::Generic {
                base: Box::new(TypeRef::id("java.util.List")),
                args: vec![TypeRef::id("java.lang.String")],
            }),
            dimensions: 1,
        };
        assert_eq!(normalize_type_for_signature(&arr), "java.util.List[]");
    }

    #[test]
    fn normalize_wildcard_and_unknown() {
        assert_eq!(
            normalize_type_for_signature(&TypeRef::Wildcard {
                bound: None,
                is_upper_bound: true
            }),
            "?"
        );
        assert_eq!(normalize_type_for_signature(&TypeRef::Unknown), "?");
    }

    // -- varargs_to_array_type --

    #[test]
    fn varargs_converts_non_array_to_array() {
        let input = TypeRef::id("java.lang.String");
        let result = varargs_to_array_type(&input);
        assert_eq!(
            result,
            TypeRef::Array {
                element: Box::new(TypeRef::id("java.lang.String")),
                dimensions: 1
            }
        );
    }

    #[test]
    fn varargs_preserves_existing_array() {
        let input = TypeRef::Array {
            element: Box::new(TypeRef::raw("int")),
            dimensions: 1,
        };
        let result = varargs_to_array_type(&input);
        assert_eq!(result, input);
    }

    // -- build_java_method_name --

    #[test]
    fn build_no_params() {
        let signed = build_java_method_name("target", &[]);
        assert_eq!(signed, "target()");
    }

    #[test]
    fn build_single_primitive_param() {
        let signed = build_java_method_name("target", &[TypeRef::raw("int")]);
        assert_eq!(signed, "target(int)");
    }

    #[test]
    fn build_multiple_params() {
        let signed = build_java_method_name("target", &[TypeRef::raw("int"), TypeRef::raw("int")]);
        assert_eq!(signed, "target(int,int)");
    }

    #[test]
    fn build_generic_param_erased() {
        let signed = build_java_method_name(
            "add",
            &[TypeRef::Generic {
                base: Box::new(TypeRef::id("java.util.List")),
                args: vec![TypeRef::id("java.lang.String")],
            }],
        );
        assert_eq!(signed, "add(java.util.List)");
    }

    #[test]
    fn build_constructor() {
        let signed = build_java_method_name("<init>", &[TypeRef::raw("int")]);
        assert_eq!(signed, "<init>(int)");
    }

    // -- End-to-end: build_java_method_name → build_member_fqn → parse_method_signature --

    #[test]
    fn roundtrip_java_method_fqn() {
        let signed = build_java_method_name(
            "target",
            &[TypeRef::raw("int"), TypeRef::id("java.lang.String")],
        );
        let fqn = build_member_fqn("com.example.A", &signed);
        assert_eq!(fqn, "com.example.A#target(int,java.lang.String)");

        let sig = parse_method_signature(&fqn).unwrap();
        assert_eq!(sig.owner, "com.example.A");
        assert_eq!(sig.name, "target");
        assert_eq!(sig.params, "int,java.lang.String");
    }

    #[test]
    fn two_java_overloads_produce_different_fqns() {
        let fqn1 = build_member_fqn(
            "com.example.A",
            &build_java_method_name("target", &[TypeRef::raw("int")]),
        );
        let fqn2 = build_member_fqn(
            "com.example.A",
            &build_java_method_name("target", &[TypeRef::raw("int"), TypeRef::raw("int")]),
        );
        assert_ne!(fqn1, fqn2);
        assert_eq!(fqn1, "com.example.A#target(int)");
        assert_eq!(fqn2, "com.example.A#target(int,int)");
    }
}
