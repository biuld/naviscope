use naviscope_api::models::NodeKind;
use naviscope_plugin::NamingConvention;

#[derive(Debug, Clone, Copy, Default)]
pub struct JavaNamingConvention;

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
