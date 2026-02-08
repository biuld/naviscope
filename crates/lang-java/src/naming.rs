use naviscope_api::models::NodeKind;
use naviscope_plugin::NamingConvention;

/// Separator used between a type and its members (methods, fields, constructors).
pub const MEMBER_SEPARATOR: char = '#';

/// Separator used between packages and between package/class.
pub const TYPE_SEPARATOR: char = '.';

#[derive(Debug, Clone, Copy, Default)]
pub struct JavaNamingConvention;

impl JavaNamingConvention {
    /// Build a fully qualified name for a member (method, field, or constructor).
    ///
    /// # Examples
    /// ```ignore
    /// build_member_fqn("com.example.MyClass", "myMethod") => "com.example.MyClass#myMethod"
    /// build_member_fqn("com.example.MyClass", "myField") => "com.example.MyClass#myField"
    /// ```
    pub fn build_member_fqn(type_fqn: &str, member_name: &str) -> String {
        format!("{}{}{}", type_fqn, MEMBER_SEPARATOR, member_name)
    }

    /// Parse a member FQN into (type_fqn, member_name).
    ///
    /// Returns `None` if the FQN does not contain a member separator.
    ///
    /// # Examples
    /// ```ignore
    /// parse_member_fqn("com.example.MyClass#myMethod") => Some(("com.example.MyClass", "myMethod"))
    /// parse_member_fqn("com.example.MyClass") => None
    /// ```
    pub fn parse_member_fqn(fqn: &str) -> Option<(&str, &str)> {
        fqn.rfind(MEMBER_SEPARATOR)
            .map(|pos| (&fqn[..pos], &fqn[pos + 1..]))
    }

    /// Check if an FQN represents a member (method, field, constructor).
    pub fn is_member_fqn(fqn: &str) -> bool {
        fqn.contains(MEMBER_SEPARATOR)
    }

    /// Extract the type FQN from a member FQN.
    ///
    /// If the FQN is already a type FQN (no member separator), returns the original.
    pub fn extract_type_fqn(fqn: &str) -> &str {
        Self::parse_member_fqn(fqn)
            .map(|(type_fqn, _)| type_fqn)
            .unwrap_or(fqn)
    }

    /// Extract the member name from a member FQN.
    ///
    /// Returns `None` if the FQN is not a member FQN.
    pub fn extract_member_name(fqn: &str) -> Option<&str> {
        Self::parse_member_fqn(fqn).map(|(_, member)| member)
    }

    /// Get the appropriate separator for the given intent.
    ///
    /// Members (Method, Field) use `#`, others use `.`.
    /// Note: Constructor is represented by SymbolIntent::Method.
    pub fn separator_for_intent(intent: naviscope_api::models::SymbolIntent) -> char {
        use naviscope_api::models::SymbolIntent;
        match intent {
            SymbolIntent::Method | SymbolIntent::Field => MEMBER_SEPARATOR,
            _ => TYPE_SEPARATOR,
        }
    }
}

impl NamingConvention for JavaNamingConvention {
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
        // Java Logic:
        // 1. Split by '.' (Package/Class separator)
        // 2. But wait! Inner classes might use '$' in bytecode but '.' in source FQN.
        //    And methods use '#' in our graph convention (from JavaParser::get_node_id_for_definition?)
        //    The string coming in here is likely a "Source FQN" (dot separated).

        let mut result = Vec::new();

        // Handle '#' for methods/fields if present (e.g. from existing graph ID)
        let (path_part, member_part) = if let Some(hash_pos) = fqn.find('#') {
            (&fqn[..hash_pos], Some(&fqn[hash_pos + 1..]))
        } else {
            (fqn, None)
        };

        // Split the path part (Packages/Classes)
        let parts: Vec<&str> = path_part.split(|c| c == '.' || c == '$').collect();
        for part in parts.iter() {
            if part.is_empty() {
                continue;
            }

            // Heuristic: Uppercase = Class, Lowercase = Package
            // This is not perfect but standard Java convention.
            let is_uppercase = part.chars().next().map_or(false, |c| c.is_uppercase());
            let kind = if is_uppercase {
                NodeKind::Class
            } else {
                NodeKind::Package
            };
            result.push((kind, part.to_string()));
        }

        // Handle member part
        if let Some(member) = member_part {
            let kind = heuristic_leaf_kind.unwrap_or(NodeKind::Method);
            result.push((kind, member.to_string()));
        }

        result
    }
}
