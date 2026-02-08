use naviscope_api::models::graph::NodeKind;
use naviscope_api::models::symbol::{FqnId, FqnReader};
use std::fmt::Debug;

/// Separator used between a type and its members (methods, fields, constructors).
pub const MEMBER_SEPARATOR: char = '#';

/// Separator used between packages and between package/class.
pub const TYPE_SEPARATOR: char = '.';

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

        // 3. Add member part if present
        if let Some(member) = member_part {
            // If parsed as member, use Method as default, or heuristic if provided
            // Note: Heuristic for member is usually Method, but could be Field.
            let kind = heuristic_leaf_kind.unwrap_or(NodeKind::Method);
            result.push((kind, member.to_string()));
        }

        result
    }
}
