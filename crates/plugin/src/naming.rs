use naviscope_api::models::graph::NodeKind;
use std::fmt::Debug;

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
    fn render_fqn(
        &self,
        id: naviscope_api::models::symbol::FqnId,
        reader: &dyn naviscope_api::models::symbol::FqnReader,
    ) -> String {
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
}

/// A default "Dot" convention (e.g. for Java/Python-ish languages).
/// It assumes "Package" -> "Class" -> "Leaf".
#[derive(Debug, Default)]
pub struct DotPathConvention;

impl NamingConvention for DotPathConvention {
    fn separator(&self) -> &str {
        "."
    }

    fn parse_fqn(
        &self,
        fqn: &str,
        heuristic_leaf_kind: Option<NodeKind>,
    ) -> Vec<(NodeKind, String)> {
        // Simple default splitting
        let parts: Vec<&str> = fqn.split('.').collect();
        let mut result = Vec::with_capacity(parts.len());

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;
            let kind = if is_last {
                heuristic_leaf_kind.clone().unwrap_or(NodeKind::Class)
            } else {
                NodeKind::Package
            };
            result.push((kind, part.to_string()));
        }
        result
    }
}
