use super::JavaParser;

use std::collections::HashSet;
use tree_sitter::Node;
use tree_sitter::StreamingIterator;

impl JavaParser {
    pub fn get_node_id_for_definition(
        &self,
        name_node: &Node,
        source: &str,
        pkg: Option<&str>,
        kind: naviscope_api::models::graph::NodeKind,
    ) -> naviscope_api::models::symbol::NodeId {
        let mut parts = Vec::new();

        // Add package if exists (as flat prefixes for now, or we can make package structured too if we split it)
        // For simplicity, let's treat package as the root part(s).
        // If pkg is "com.example", we can either have one Package("com.example") or multiple.
        // The standard convention for FQN usually treats package as one block or multiple.
        // Let's use one block for package for now to match current string behavior efficiently,
        // or better, split it. Splitting is "more structured".
        // But `NodeId::Structured` expects (Kind, Name).

        if let Some(p) = pkg {
            if !p.is_empty() {
                // Split package into structural parts to ensure FQN traversal works
                // e.g. "com.example" -> (Package, "com"), (Package, "example")
                for part in p.split('.') {
                    parts.push((
                        naviscope_api::models::graph::NodeKind::Package,
                        part.to_string(),
                    ));
                }
            }
        }

        // Collect parents
        let mut hierarchy = Vec::new();
        let mut curr = *name_node;
        let mut seen_ids = HashSet::new();
        seen_ids.insert(name_node.id());

        // Add self
        let self_name = name_node
            .utf8_text(source.as_bytes())
            .unwrap_or_default()
            .to_string();

        // We accumulate parents then reverse
        while let Some(parent) = self.find_next_enclosing_definition(curr) {
            let p_kind_str = parent.kind();

            // Map TS kind to NodeKind
            let p_node_kind = match p_kind_str {
                "class_declaration" => Some(naviscope_api::models::graph::NodeKind::Class),
                "interface_declaration" => Some(naviscope_api::models::graph::NodeKind::Interface),
                "enum_declaration" => Some(naviscope_api::models::graph::NodeKind::Enum),
                "annotation_type_declaration" => {
                    Some(naviscope_api::models::graph::NodeKind::Annotation)
                }
                "method_declaration" => Some(naviscope_api::models::graph::NodeKind::Method),
                "constructor_declaration" => {
                    Some(naviscope_api::models::graph::NodeKind::Constructor)
                }
                _ => None,
            };

            if let Some(pk) = p_node_kind {
                if let Some(n_node) = parent.child_by_field_name("name") {
                    if seen_ids.insert(n_node.id()) {
                        if let Ok(n_text) = n_node.utf8_text(source.as_bytes()) {
                            let id_pk = match pk {
                                naviscope_api::models::graph::NodeKind::Interface
                                | naviscope_api::models::graph::NodeKind::Enum
                                | naviscope_api::models::graph::NodeKind::Annotation => {
                                    naviscope_api::models::graph::NodeKind::Class
                                }
                                _ => pk,
                            };
                            hierarchy.push((id_pk, n_text.to_string()));
                        }
                    }
                }
            }
            curr = parent;
        }

        hierarchy.reverse();
        parts.extend(hierarchy);

        // Add self at the end
        // STABILITY NOTE: For Java, we use NodeKind::Class for all Type-like entities
        // in the ID to ensure cross-file references (which often don't know the exact kind)
        // can resolve correctly. The actual node.kind will still be accurate.
        let id_kind = match kind {
            naviscope_api::models::graph::NodeKind::Interface
            | naviscope_api::models::graph::NodeKind::Enum
            | naviscope_api::models::graph::NodeKind::Annotation => {
                naviscope_api::models::graph::NodeKind::Class
            }
            _ => kind,
        };
        parts.push((id_kind, self_name));

        naviscope_api::models::symbol::NodeId::Structured(parts)
    }

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

        // 1. If it looks like an absolute FQN, don't scan the tree or prefix with current package
        if type_name.contains('.') {
            let first = type_name.split('.').next().unwrap_or("");
            if !first.is_empty() && first.chars().next().unwrap_or(' ').is_lowercase() {
                // Heuristic: starts with lowercase, likely a package name (com, org, java, ...)
                return Some(type_name.to_string());
            }
        }

        // 2. Check if it's defined in the current file (handling inner classes)
        // We scan the tree for class/interface/enum definitions with this name.
        let mut cursor = tree_sitter::QueryCursor::new();
        let mut matches =
            cursor.matches(&self.definition_query, tree.root_node(), source.as_bytes());

        while let Some(mat) = matches.next() {
            for cap in mat.captures {
                // Check if capture is a name node
                if cap.index == self.indices.class_name
                    || cap.index == self.indices.inter_name
                    || cap.index == self.indices.enum_name
                    || cap.index == self.indices.annotation_name
                {
                    if let Ok(name) = cap.node.utf8_text(source.as_bytes()) {
                        if name == type_name {
                            // Found a definition! Get its FQN.
                            return Some(self.get_fqn_for_definition(
                                &cap.node,
                                source,
                                pkg.as_deref(),
                            ));
                        }
                    }
                }
            }
        }

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

            // Heuristic: If first part starts with lowercase, it's likely a package (com, org, java, ...)
            // We assume it's an absolute FQN if it's not found in imports.
            if !first_part.is_empty() && first_part.chars().next().unwrap_or(' ').is_lowercase() {
                return Some(type_name.to_string());
            }

            // If the first part is already the current package, don't recurse
            // (Actually this is rare for first part unless package is just one level)
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

    pub fn tree_sitter_kind_to_node_kind(
        ts_kind: &str,
    ) -> Option<naviscope_api::models::graph::NodeKind> {
        match ts_kind {
            "class_declaration" => Some(naviscope_api::models::graph::NodeKind::Class),
            "interface_declaration" => Some(naviscope_api::models::graph::NodeKind::Interface),
            "enum_declaration" => Some(naviscope_api::models::graph::NodeKind::Enum),
            "annotation_type_declaration" => {
                Some(naviscope_api::models::graph::NodeKind::Annotation)
            }
            "method_declaration" => Some(naviscope_api::models::graph::NodeKind::Method),
            "constructor_declaration" => Some(naviscope_api::models::graph::NodeKind::Constructor),
            "field_declaration" => Some(naviscope_api::models::graph::NodeKind::Field),
            _ => None,
        }
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
