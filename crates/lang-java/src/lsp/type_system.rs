use naviscope_api::models::SymbolResolution;
use naviscope_plugin::ReferenceCheckService;
use naviscope_plugin::graph::CodeGraph;

pub struct JavaTypeSystem;

impl JavaTypeSystem {
    pub fn new() -> Self {
        Self
    }
}

impl ReferenceCheckService for JavaTypeSystem {
    fn is_reference_to(
        &self,
        graph: &dyn CodeGraph,
        candidate: &SymbolResolution,
        target: &SymbolResolution,
    ) -> bool {
        // Core Java identity and inheritance logic
        if candidate == target {
            return true;
        }

        // Handle method overrides/implementations
        let c_fqn = candidate.fqn();
        let t_fqn = target.fqn();

        if let (Some(c_fqn), Some(t_fqn)) = (c_fqn, t_fqn) {
            self.check_inheritance_match(graph, c_fqn, t_fqn)
        } else {
            false
        }
    }
}

impl JavaTypeSystem {
    fn check_inheritance_match(
        &self,
        graph: &dyn CodeGraph,
        candidate_fqn: &str,
        target_fqn: &str,
    ) -> bool {
        // Logic: if both are members (contain '#'), compare names and check if classes match inheritance
        use naviscope_plugin::naming::parse_member_fqn;

        if let (Some((c_type, c_member)), Some((t_type, t_member))) = (
            parse_member_fqn(candidate_fqn),
            parse_member_fqn(target_fqn),
        ) {
            // Compare: if both are signed, require exact match; otherwise simple-name match
            let c_has_sig = naviscope_plugin::naming::has_method_signature(c_member);
            let t_has_sig = naviscope_plugin::naming::has_method_signature(t_member);
            let names_match = if c_has_sig && t_has_sig {
                c_member == t_member
            } else {
                naviscope_plugin::naming::extract_simple_name(c_member)
                    == naviscope_plugin::naming::extract_simple_name(t_member)
            };
            if names_match {
                // Member names match, check if classes are related
                return self.is_subtype(graph, c_type, t_type)
                    || self.is_subtype(graph, t_type, c_type);
            }
        }

        false
    }
}
