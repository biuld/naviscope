use super::JavaParser;
use std::collections::HashSet;
use tree_sitter::Node;

impl JavaParser {
    /// Gets the full FQN for a definition node.
    pub fn get_fqn_for_definition(
        &self,
        name_node: &Node,
        source: &str,
        pkg: Option<&str>,
    ) -> String {
        let mut parts = Vec::new();
        let mut curr = *name_node;
        let mut seen_ids = HashSet::new();

        parts.push(
            name_node
                .utf8_text(source.as_bytes())
                .unwrap_or_default()
                .to_string(),
        );
        seen_ids.insert(name_node.id());

        while let Some(parent) = self.find_next_enclosing_definition(curr) {
            let kind = parent.kind();
            // FQN for Java elements should only include Packages and Classes/Interfaces/Enums.
            // Methods and Constructors should be skipped when calculating the FQN of nested elements.
            if kind.contains("class")
                || kind.contains("interface")
                || kind.contains("enum")
                || kind.contains("annotation")
                || kind == "variable_declarator"
            {
                if let Some(n_node) = parent.child_by_field_name("name") {
                    if seen_ids.insert(n_node.id()) {
                        if let Ok(n_text) = n_node.utf8_text(source.as_bytes()) {
                            parts.push(n_text.to_string());
                        }
                    }
                }
            }
            curr = parent;
        }

        parts.reverse();
        let mut fqn = if let Some(p) = pkg {
            p.to_string()
        } else {
            String::new()
        };
        for p in parts {
            if !fqn.is_empty() {
                fqn.push('.');
            }
            fqn.push_str(&p);
        }
        fqn
    }

    /// Returns a list of FQNs for all enclosing classes from inner to outer.
    pub fn get_enclosing_class_fqns(
        &self,
        node: &Node,
        source: &str,
        pkg: Option<&str>,
    ) -> Vec<String> {
        let mut fqns = Vec::new();
        let mut curr = *node;

        // If the current node is the name of a definition, start searching from the definition itself
        // to avoid including the current definition in the enclosing class list.
        if let Some(parent) = curr.parent() {
            if Self::is_definition_node(parent.kind()) {
                if let Some(name_node) = parent.child_by_field_name("name") {
                    if name_node.id() == node.id() {
                        curr = parent;
                    }
                }
            }
        }

        while let Some(container) = self.find_next_enclosing_definition(curr) {
            let kind = container.kind();
            if kind.contains("class") || kind.contains("interface") || kind.contains("enum") {
                if let Some(name_node) = container.child_by_field_name("name") {
                    fqns.push(self.get_fqn_for_definition(&name_node, source, pkg));
                }
            }
            curr = container;
        }
        fqns
    }

    pub fn resolve_type_name_to_fqn(
        &self,
        type_name: &str,
        tree: &tree_sitter::Tree,
        source: &str,
    ) -> Option<String> {
        let (pkg, imports) = self.extract_package_and_imports(tree, source);
        self.resolve_type_name_to_fqn_data(type_name, pkg.as_deref(), &imports)
    }

    pub fn resolve_type_name_to_fqn_data(
        &self,
        type_name: &str,
        package: Option<&str>,
        imports: &[String],
    ) -> Option<String> {
        // 1. Check if it's a primitive type
        const PRIMITIVES: &[&str] = &[
            "int", "long", "short", "byte", "float", "double", "boolean", "char", "void",
        ];
        if PRIMITIVES.contains(&type_name) {
            return Some(type_name.to_string());
        }

        // 2. Handle dotted names (e.g. Config.KEY or com.example.Config)
        if type_name.contains('.') {
            let parts: Vec<&str> = type_name.split('.').collect();
            let first_part = parts[0];

            // If the first part is already the current package, don't recurse
            if let Some(p) = package {
                if first_part == p {
                    return Some(type_name.to_string());
                }
            }

            // Try to resolve the first part as a type
            if let Some(first_fqn) =
                self.resolve_type_name_to_fqn_data(first_part, package, imports)
            {
                if first_fqn != first_part {
                    // It was resolved to something else (e.g. com.example.Config)
                    let mut full_fqn = first_fqn;
                    for part in &parts[1..] {
                        full_fqn.push('.');
                        full_fqn.push_str(part);
                    }
                    return Some(full_fqn);
                }
            }
            return Some(type_name.to_string());
        }

        // 3. Precise imports
        for imp in imports {
            if imp.ends_with(&format!(".{}", type_name)) {
                return Some(imp.clone());
            }
        }

        // 4. Wildcard imports (e.g., import java.util.*;)
        // 5. java.lang (implicit import)
        const JAVA_LANG_CLASSES: &[&str] = &[
            "String",
            "Object",
            "Integer",
            "Long",
            "Double",
            "Float",
            "Boolean",
            "Byte",
            "Character",
            "Short",
            "Exception",
            "RuntimeException",
            "Throwable",
            "Error",
            "Thread",
            "System",
            "Class",
            "Iterable",
            "Runnable",
            "Comparable",
            "SuppressWarnings",
            "Override",
            "Deprecated",
        ];
        if JAVA_LANG_CLASSES.contains(&type_name) {
            return Some(format!("java.lang.{}", type_name));
        }

        // 6. Current package
        if let Some(p) = package {
            // Don't append package if it's already an FQN starting with this package or other known packages
            if type_name.starts_with(&(p.to_string() + "."))
                || type_name.starts_with("java.")
                || type_name.starts_with("javax.")
                || type_name.starts_with("com.")
                || type_name.starts_with("org.")
                || type_name.starts_with("net.")
            {
                return Some(type_name.to_string());
            }
            return Some(format!("{}.{}", p, type_name));
        }

        Some(type_name.to_string())
    }

    pub(crate) fn is_definition_node(kind: &str) -> bool {
        matches!(
            kind,
            "class_declaration"
                | "interface_declaration"
                | "enum_declaration"
                | "annotation_type_declaration"
                | "method_declaration"
                | "constructor_declaration"
                | "field_declaration"
                | "variable_declarator"
        )
    }

    pub fn find_next_enclosing_definition<'a>(&self, node: Node<'a>) -> Option<Node<'a>> {
        let mut curr = node;
        while let Some(parent) = curr.parent() {
            if Self::is_definition_node(parent.kind()) {
                return Some(parent);
            }
            curr = parent;
        }
        None
    }
}
