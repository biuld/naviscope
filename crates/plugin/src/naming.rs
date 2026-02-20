use naviscope_api::models::graph::NodeKind;
use naviscope_api::models::symbol::{FqnId, FqnReader};
use std::fmt::Debug;

/// Separator used between a type and its members (methods, fields, constructors).
pub const MEMBER_SEPARATOR: char = '#';

/// Separator used between packages and between package/class.
pub const TYPE_SEPARATOR: char = '.';

// ---------------------------------------------------------------------------
// Name-level member FQN utilities (existing)
// ---------------------------------------------------------------------------

/// Build a fully qualified name for a member (method, field, or constructor).
pub fn build_member_fqn(type_fqn: &str, member_name: &str) -> String {
    format!("{}{}{}", type_fqn, MEMBER_SEPARATOR, member_name)
}

/// Parse a member FQN into (type_fqn, member_name).
/// Returns `None` if the FQN does not contain a member separator.
pub fn parse_member_fqn(fqn: &str) -> Option<(&str, &str)> {
    fqn.rfind(MEMBER_SEPARATOR)
        .map(|pos| (&fqn[..pos], &fqn[pos + 1..]))
}

/// Check if an FQN represents a member (method, field, constructor).
pub fn is_member_fqn(fqn: &str) -> bool {
    fqn.contains(MEMBER_SEPARATOR)
}

/// Extract the type FQN from a member FQN.
pub fn extract_type_fqn(fqn: &str) -> &str {
    parse_member_fqn(fqn)
        .map(|(type_fqn, _)| type_fqn)
        .unwrap_or(fqn)
}

/// Extract the member name from a member FQN.
pub fn extract_member_name(fqn: &str) -> Option<&str> {
    parse_member_fqn(fqn).map(|(_, member)| member)
}

// ---------------------------------------------------------------------------
// Signature-level method FQN utilities (cross-language)
//
// These operate on the project's FQN format convention (`Owner#name(params)`)
// and are language-agnostic. Language-specific type normalization (e.g. Java
// generic erasure) belongs in the respective `lang-*` crate.
// ---------------------------------------------------------------------------

/// Format a method member name with its parameter signature.
///
/// Takes a method name and **already-normalized** parameter type strings.
/// The normalization of types (e.g. generic erasure, varargs→array) is
/// language-specific and should be done by the caller.
///
/// # Examples
///
/// - `format_method_name("target", &[])` → `"target()"`
/// - `format_method_name("target", &["int"])` → `"target(int)"`
/// - `format_method_name("target", &["int", "java.lang.String"])` → `"target(int,java.lang.String)"`
///
/// The result can be passed directly to [`build_member_fqn`]:
/// ```
/// use naviscope_plugin::naming::{build_member_fqn, format_method_name};
///
/// let signed = format_method_name("target", &["int"]);
/// let fqn = build_member_fqn("com.example.A", &signed);
/// assert_eq!(fqn, "com.example.A#target(int)");
/// ```
pub fn format_method_name(name: &str, normalized_param_types: &[&str]) -> String {
    format!("{}({})", name, normalized_param_types.join(","))
}

/// Parsed components of a signature-level method FQN.
///
/// Produced by [`parse_method_signature`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodSignature<'a> {
    /// Owner type FQN (e.g., `"com.example.A"`)
    pub owner: &'a str,
    /// Method simple name (e.g., `"target"`)
    pub name: &'a str,
    /// Raw parameter string inside parentheses (e.g., `"int,java.lang.String"`).
    /// Empty string for no-arg methods.
    pub params: &'a str,
}

/// Parse a signature-level method FQN into its components.
///
/// Returns `None` if the FQN is not a signed method FQN (no `#` or no parentheses).
///
/// # Examples
///
/// ```
/// use naviscope_plugin::naming::parse_method_signature;
///
/// let sig = parse_method_signature("com.example.A#target(int)").unwrap();
/// assert_eq!(sig.owner, "com.example.A");
/// assert_eq!(sig.name, "target");
/// assert_eq!(sig.params, "int");
///
/// let sig = parse_method_signature("com.example.A#target()").unwrap();
/// assert_eq!(sig.params, "");
///
/// // Not a signed method FQN:
/// assert!(parse_method_signature("com.example.A#field").is_none());
/// assert!(parse_method_signature("com.example.A").is_none());
/// ```
pub fn parse_method_signature(fqn: &str) -> Option<MethodSignature<'_>> {
    let (owner, member) = parse_member_fqn(fqn)?;
    let paren_start = member.find('(')?;
    let paren_end = member.rfind(')')?;
    if paren_end <= paren_start {
        return None;
    }
    let name = &member[..paren_start];
    let params = &member[paren_start + 1..paren_end];
    Some(MethodSignature {
        owner,
        name,
        params,
    })
}

/// Check if a member FQN includes a method signature (has parentheses after `#`).
///
/// - `"com.example.A#target(int)"` → `true`
/// - `"com.example.A#target()"` → `true`
/// - `"com.example.A#field"` → `false`
/// - `"com.example.A"` → `false`
pub fn has_method_signature(fqn: &str) -> bool {
    parse_method_signature(fqn).is_some()
}

/// Extract the simple name from a member part that may include a signature.
///
/// This is useful for display purposes — stripping the parameter list while keeping
/// the method name.
///
/// - `"target(int)"` → `"target"`
/// - `"target"` → `"target"`
/// - `"<init>(int)"` → `"<init>"`
pub fn extract_simple_name(member: &str) -> &str {
    member.find('(').map(|i| &member[..i]).unwrap_or(member)
}

// ---------------------------------------------------------------------------
// NamingConvention trait
// ---------------------------------------------------------------------------

/// Defines language-specific naming rules for Fully Qualified Names (FQNs).
/// This trait allows the core system to parse flat strings into structured paths
/// based on language semantics (separators, nesting rules, etc.).
pub trait NamingConvention: Send + Sync + Debug {
    /// The primary separator (e.g., "." for Java, "::" for Rust).
    fn separator(&self) -> &str;

    /// Get the separator between a parent node and a child node based on their kinds.
    fn get_separator(&self, _parent: NodeKind, _child: NodeKind) -> &str {
        self.separator()
    }

    /// Parse a flat FQN string into structured segments.
    ///
    /// # Arguments
    /// * `fqn` - The raw FQN string (e.g. "com.example.Class#method")
    /// * `heuristic_leaf_kind` - An optional hint about what the leaf node might be.
    ///
    /// # Returns
    /// A list of (NodeKind, String) tuples representing the path from root to leaf.
    fn parse_fqn(
        &self,
        fqn: &str,
        heuristic_leaf_kind: Option<NodeKind>,
    ) -> Vec<(NodeKind, String)>;

    /// Render a structured FQN into a string using this convention.
    fn render_fqn(&self, id: FqnId, reader: &dyn FqnReader) -> String {
        let mut parts = Vec::new();
        let mut current = Some(id);

        while let Some(curr_id) = current {
            if let Some(node) = reader.resolve_node(curr_id) {
                current = node.parent;
                parts.push(node);
            } else {
                break;
            }
        }

        parts.reverse();

        let mut result = String::new();
        for (i, node) in parts.iter().enumerate() {
            let name = reader.resolve_atom(node.name);
            result.push_str(name);

            if i < parts.len() - 1 {
                let next_node = &parts[i + 1];
                result.push_str(self.get_separator(node.kind.clone(), next_node.kind.clone()));
            }
        }
        result
    }

    /// Get the member separator character (default '#').
    fn member_separator(&self) -> char {
        MEMBER_SEPARATOR
    }

    /// Build a fully qualified name for a member using this convention.
    fn build_member_fqn(&self, type_fqn: &str, member_name: &str) -> String {
        build_member_fqn(type_fqn, member_name)
    }

    /// Parse a member FQN into (type_fqn, member_name) using this convention.
    fn parse_member_fqn<'a>(&self, fqn: &'a str) -> Option<(&'a str, &'a str)> {
        parse_member_fqn(fqn)
    }

    /// Check if an FQN represents a member using this convention.
    fn is_member_fqn(&self, fqn: &str) -> bool {
        is_member_fqn(fqn)
    }
}

/// A standard naming convention suitable for most polyglot scenarios.
/// It uses `.` for hierarchy/types and `#` for members.
/// This implementation unifies behavior across languages unless specific overrides are needed.
#[derive(Debug, Default, Clone, Copy)]
pub struct StandardNamingConvention;

impl NamingConvention for StandardNamingConvention {
    fn separator(&self) -> &str {
        "."
    }

    fn get_separator(&self, parent: NodeKind, child: NodeKind) -> &str {
        match (parent, child) {
            (
                NodeKind::Class | NodeKind::Interface | NodeKind::Enum | NodeKind::Annotation,
                NodeKind::Method | NodeKind::Field | NodeKind::Constructor,
            ) => "#",
            _ => ".",
        }
    }

    fn parse_fqn(
        &self,
        fqn: &str,
        heuristic_leaf_kind: Option<NodeKind>,
    ) -> Vec<(NodeKind, String)> {
        // 1. Check for standard member separator first
        let (type_part, member_part) = parse_member_fqn(fqn)
            .map(|(t, m)| (t, Some(m)))
            .unwrap_or((fqn, None));

        // 2. Split the type part by '.'
        let parts: Vec<&str> = type_part.split(TYPE_SEPARATOR).collect();
        let mut result =
            Vec::with_capacity(parts.len() + if member_part.is_some() { 1 } else { 0 });

        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() {
                continue;
            }
            // If we have a member part, then the last part of type_path is likely a Class/Type.
            // If not, we use the heuristic or default to Class.
            let is_last_type_part = i == parts.len() - 1;
            let kind = if is_last_type_part {
                if member_part.is_some() {
                    NodeKind::Class
                } else {
                    heuristic_leaf_kind.clone().unwrap_or(NodeKind::Class)
                }
            } else {
                NodeKind::Package
            };
            result.push((kind, part.to_string()));
        }

        // 3. Add member part if present (includes signature for signed method FQNs)
        if let Some(member) = member_part {
            // If parsed as member, use Method as default, or heuristic if provided
            // Note: Heuristic for member is usually Method, but could be Field.
            let kind = heuristic_leaf_kind.unwrap_or(NodeKind::Method);
            result.push((kind, member.to_string()));
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- format_method_name --

    #[test]
    fn format_method_name_basic() {
        assert_eq!(format_method_name("foo", &[]), "foo()");
        assert_eq!(format_method_name("foo", &["A"]), "foo(A)");
        assert_eq!(format_method_name("foo", &["A", "B"]), "foo(A,B)");
    }

    #[test]
    fn format_method_name_composes_with_build_member_fqn() {
        let signed = format_method_name("bar", &["X"]);
        let fqn = build_member_fqn("pkg.Owner", &signed);
        assert_eq!(fqn, "pkg.Owner#bar(X)");
    }

    // -- parse_method_signature --

    #[test]
    fn parse_no_params() {
        let sig = parse_method_signature("pkg.Owner#foo()").unwrap();
        assert_eq!(sig.owner, "pkg.Owner");
        assert_eq!(sig.name, "foo");
        assert_eq!(sig.params, "");
    }

    #[test]
    fn parse_single_param() {
        let sig = parse_method_signature("pkg.Owner#foo(T1)").unwrap();
        assert_eq!(sig.owner, "pkg.Owner");
        assert_eq!(sig.name, "foo");
        assert_eq!(sig.params, "T1");
    }

    #[test]
    fn parse_multiple_params() {
        let sig = parse_method_signature("pkg.Owner#foo(T1,T2)").unwrap();
        assert_eq!(sig.owner, "pkg.Owner");
        assert_eq!(sig.name, "foo");
        assert_eq!(sig.params, "T1,T2");
    }

    #[test]
    fn parse_not_signed_field() {
        assert!(parse_method_signature("pkg.Owner#field").is_none());
    }

    #[test]
    fn parse_not_a_member() {
        assert!(parse_method_signature("pkg.Owner").is_none());
    }

    // -- has_method_signature --

    #[test]
    fn has_signature_true_cases() {
        assert!(has_method_signature("pkg.Owner#foo()"));
        assert!(has_method_signature("pkg.Owner#foo(T1)"));
        assert!(has_method_signature("pkg.Owner#foo(T1,T2)"));
    }

    #[test]
    fn has_signature_false_cases() {
        assert!(!has_method_signature("pkg.Owner#field"));
        assert!(!has_method_signature("pkg.Owner"));
        assert!(!has_method_signature("T1"));
    }

    // -- extract_simple_name --

    #[test]
    fn extract_simple_name_with_signature() {
        assert_eq!(extract_simple_name("foo(T1)"), "foo");
        assert_eq!(extract_simple_name("foo()"), "foo");
        assert_eq!(extract_simple_name("foo(T1,T2)"), "foo");
    }

    #[test]
    fn extract_simple_name_without_signature() {
        assert_eq!(extract_simple_name("foo"), "foo");
        assert_eq!(extract_simple_name("field"), "field");
    }

    // -- Interaction with existing utilities --

    #[test]
    fn parse_member_fqn_works_with_signed_method() {
        let (owner, member) = parse_member_fqn("pkg.Owner#foo(T1)").unwrap();
        assert_eq!(owner, "pkg.Owner");
        assert_eq!(member, "foo(T1)");
        assert_eq!(extract_simple_name(member), "foo");
    }

    #[test]
    fn roundtrip_format_then_parse() {
        let signed = format_method_name("foo", &["T1", "T2"]);
        let fqn = build_member_fqn("pkg.Owner", &signed);
        assert_eq!(fqn, "pkg.Owner#foo(T1,T2)");

        let sig = parse_method_signature(&fqn).unwrap();
        assert_eq!(sig.owner, "pkg.Owner");
        assert_eq!(sig.name, "foo");
        assert_eq!(sig.params, "T1,T2");
    }

    // -- StandardNamingConvention compatibility --

    #[test]
    fn standard_convention_parses_signed_method_fqn() {
        let conv = StandardNamingConvention;
        let parts = conv.parse_fqn("pkg.Owner#foo(T1)", None);
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], (NodeKind::Package, "pkg".to_string()));
        assert_eq!(parts[1], (NodeKind::Class, "Owner".to_string()));
        assert_eq!(parts[2], (NodeKind::Method, "foo(T1)".to_string()));
    }

    #[test]
    fn two_overloads_produce_distinct_fqn_paths() {
        let conv = StandardNamingConvention;
        let p1 = conv.parse_fqn("pkg.Owner#foo(T1)", None);
        let p2 = conv.parse_fqn("pkg.Owner#foo(T1,T2)", None);
        assert_ne!(p1.last(), p2.last());
    }
}
