use crate::inference::{
    InheritanceProvider, MemberInfo, MemberProvider, TypeInfo, TypeProvider, TypeResolutionContext,
};

/// A TypeProvider that uses heuristics to resolve type names without a backing graph.
///
/// This is primarily used during indexing when the graph is incomplete,
/// allowing for optimistic resolution of FQNs based on conventions and imports.
pub struct HeuristicAdapter;

impl TypeProvider for HeuristicAdapter {
    fn get_type_info(&self, _fqn: &str) -> Option<TypeInfo> {
        None
    }

    fn resolve_type_name(&self, type_name: &str, ctx: &TypeResolutionContext) -> Option<String> {
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

            if !first_part.is_empty() && first_part.chars().next().unwrap_or(' ').is_lowercase() {
                return Some(type_name.to_string());
            }

            if let Some(p) = &ctx.package {
                if first_part == p {
                    return Some(type_name.to_string());
                }
            }

            if let Some(first_fqn) = self.resolve_type_name(first_part, ctx) {
                if first_fqn != first_part {
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
        for imp in &ctx.imports {
            if imp.ends_with(&format!(".{}", type_name)) {
                return Some(imp.clone());
            }
        }

        // 4. java.lang (implicit import)
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

        // 5. Current package
        if let Some(p) = &ctx.package {
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
}

impl InheritanceProvider for HeuristicAdapter {
    fn get_superclass(&self, _fqn: &str) -> Option<String> {
        None
    }

    fn get_interfaces(&self, _fqn: &str) -> Vec<String> {
        vec![]
    }

    fn walk_ancestors(&self, _fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(std::iter::empty())
    }

    fn get_direct_subtypes(&self, _fqn: &str) -> Vec<String> {
        vec![]
    }

    fn walk_descendants(&self, _fqn: &str) -> Box<dyn Iterator<Item = String> + '_> {
        Box::new(std::iter::empty())
    }
}

impl MemberProvider for HeuristicAdapter {
    fn get_members(&self, _type_fqn: &str, _member_name: &str) -> Vec<MemberInfo> {
        vec![]
    }

    fn get_all_members(&self, _type_fqn: &str) -> Vec<MemberInfo> {
        vec![]
    }
}
