use crate::JavaPlugin;
use crate::inference::adapters::CodeGraphTypeSystem;
use crate::inference::{InheritanceProvider, MemberProvider, TypeProvider, TypeResolutionContext};
use crate::resolve::context::ResolutionContext;
use naviscope_api::models::graph::EdgeType;
use naviscope_api::models::symbol::{FqnId, matches_intent};
use naviscope_api::models::{SymbolIntent, SymbolResolution, TypeRef};
use naviscope_plugin::{CodeGraph, NamingConvention, SymbolQueryService, SymbolResolveService};
use tree_sitter::Tree;

impl SymbolResolveService for JavaPlugin {
    fn resolve_at(
        &self,
        tree: &Tree,
        source: &str,
        line: usize,
        byte_col: usize,
        index: &dyn CodeGraph,
    ) -> Option<SymbolResolution> {
        let point = tree_sitter::Point::new(line, byte_col);
        let node = tree
            .root_node()
            .named_descendant_for_point_range(point, point)
            .filter(|n| {
                matches!(
                    n.kind(),
                    "identifier" | "type_identifier" | "scoped_identifier" | "this"
                )
            })?;

        let name = node.utf8_text(source.as_bytes()).ok()?.to_string();
        let context = ResolutionContext::new(node, name, index, source, tree, &self.parser);

        self.resolve_symbol_internal(&context)
    }
}

impl SymbolQueryService for JavaPlugin {
    fn find_matches(&self, index: &dyn CodeGraph, resolution: &SymbolResolution) -> Vec<FqnId> {
        match resolution {
            SymbolResolution::Local(_, _) => vec![],
            SymbolResolution::Precise(fqn, _intent) => index.resolve_fqn(fqn),
            SymbolResolution::Global(fqn) => index.resolve_fqn(fqn),
        }
    }

    fn resolve_type_of(
        &self,
        index: &dyn CodeGraph,
        resolution: &SymbolResolution,
    ) -> Vec<SymbolResolution> {
        let mut type_resolutions = Vec::new();
        let ts = CodeGraphTypeSystem::new(index);

        match resolution {
            SymbolResolution::Local(_, type_name) => {
                if let Some(tn) = type_name {
                    // Use TypeProvider from type system
                    let ctx = TypeResolutionContext::default(); // Minimal ctx for LSP verify
                    if let Some(fqn) = ts.resolve_type_name(tn, &ctx) {
                        type_resolutions.push(SymbolResolution::Precise(fqn, SymbolIntent::Type));
                    }
                }
            }
            SymbolResolution::Precise(fqn, intent) => {
                // If it's a member (Field/Method), find its type via MemberProvider
                // Use unified member FQN parsing
                if let Some((type_fqn, member_name)) = crate::naming::parse_member_fqn(fqn) {
                    if let Some(member) = ts.get_members(type_fqn, member_name).first() {
                        match &member.type_ref {
                            TypeRef::Raw(s) => type_resolutions
                                .push(SymbolResolution::Precise(s.clone(), SymbolIntent::Type)),
                            TypeRef::Id(id) => type_resolutions
                                .push(SymbolResolution::Precise(id.clone(), SymbolIntent::Type)),
                            _ => {}
                        }
                    }
                } else if *intent == SymbolIntent::Type {
                    type_resolutions.push(resolution.clone());
                } else {
                    // Fallback for cases without member separator or other global symbols
                    let fids = index.resolve_fqn(fqn);
                    for fid in fids {
                        if let Some(node) = index.get_node(fid) {
                            if matches_intent(&node.kind, SymbolIntent::Type) {
                                type_resolutions.push(resolution.clone());
                            }
                        }
                    }
                }
            }
            SymbolResolution::Global(fqn) => {
                let fids = index.resolve_fqn(fqn);
                for fid in fids {
                    if let Some(node) = index.get_node(fid) {
                        if matches_intent(&node.kind, SymbolIntent::Type) {
                            type_resolutions.push(resolution.clone());
                        }
                    }
                }
            }
        }
        type_resolutions
    }

    fn find_implementations(
        &self,
        index: &dyn CodeGraph,
        resolution: &SymbolResolution,
    ) -> Vec<FqnId> {
        let target_nodes = self.find_matches(index, resolution);
        let mut results = Vec::new();
        let ts = CodeGraphTypeSystem::new(index);

        for &node_id in &target_nodes {
            let node = match index.get_node(node_id) {
                Some(n) => n,
                None => continue,
            };

            // Check if it's a method
            let is_method = matches!(
                node.kind,
                naviscope_api::models::graph::NodeKind::Method
                    | naviscope_api::models::graph::NodeKind::Constructor
            );

            if is_method {
                // 1. Find the enclosing class/interface
                let parents = index.get_neighbors(
                    node_id,
                    naviscope_plugin::Direction::Incoming,
                    Some(EdgeType::Contains),
                );
                for parent_id in parents {
                    // 2. Find all implementations of this parent
                    use naviscope_plugin::NamingConvention;
                    let parent_fqn = crate::naming::JavaNamingConvention::default()
                        .render_fqn(parent_id, index.fqns());

                    // 3. Walk all descendants of the parent class
                    for desc_fqn in ts.walk_descendants(&parent_fqn) {
                        // 4. In each descendant class, find a member with same name
                        let method_name = index.fqns().resolve_atom(node.name);
                        if let Some(member) = ts.get_members(&desc_fqn, method_name).first() {
                            results.extend(index.resolve_fqn(&member.fqn));
                        }
                    }
                }
                continue;
            }

            // For classes/interfaces, get all descendants
            let fqn =
                crate::naming::JavaNamingConvention::default().render_fqn(node_id, index.fqns());
            for desc_fqn in ts.walk_descendants(&fqn) {
                results.extend(index.resolve_fqn(&desc_fqn));
            }
        }
        results
    }
}
