use super::CodeGraphLike;
use lsp_types::{Location, Uri};
pub use naviscope_api::models::SymbolResolution;
use naviscope_plugin::SemanticCap;
use std::collections::HashSet;

/// DiscoveryEngine bridges Meso-level graph knowledge with Micro-level file scanning.
pub struct DiscoveryEngine<'a> {
    index: &'a dyn CodeGraphLike,
    naming_conventions:
        std::collections::HashMap<String, std::sync::Arc<dyn naviscope_plugin::NamingConvention>>,
}

impl<'a> DiscoveryEngine<'a> {
    pub fn new(
        index: &'a dyn CodeGraphLike,
        naming_conventions: std::collections::HashMap<
            String,
            std::sync::Arc<dyn naviscope_plugin::NamingConvention>,
        >,
    ) -> Self {
        Self {
            index,
            naming_conventions,
        }
    }

    fn get_convention(
        &self,
        node: &crate::model::GraphNode,
    ) -> Option<&dyn naviscope_plugin::NamingConvention> {
        let lang_str = self.index.symbols().resolve(&node.lang.0);
        self.naming_conventions.get(lang_str).map(|c| c.as_ref())
    }

    /// Meso-level: Scout for candidate files that likely contain references to the given nodes.
    /// Returns a set of unique file paths.
    ///
    /// Strategy:
    /// 1. Extract all possible identifier tokens from node's FQN and name
    /// 2. Use reference_index (inverted index) to quickly find candidate files containing these tokens
    ///
    /// Note: This relies on reference_index which contains all identifier tokens found during parsing.
    /// The actual reference verification is done at micro-level using tree-sitter parsing.
    /// Meso-level: Scout for candidate files that likely contain references to the given nodes.
    /// Returns a set of unique file paths.
    ///
    /// Strategy:
    /// 1. Extract "primary" (name) and "context" (parent) tokens.
    /// 2. If context exists, use INTERSECTION of file sets to reduce candidates.
    /// 3. Fallback to primary token union if context is missing or not found.
    /// Meso-level: Scout for candidate files that likely contain references to the given nodes.
    /// Returns a set of unique file paths.
    ///
    /// Strategy:
    /// 1. Extract "primary" (name) and "context" (parent) tokens.
    /// 2. If context exists, use INTERSECTION of file sets to reduce candidates.
    /// 3. Fallback to primary token union if context is missing or not found.
    pub fn scout_references(
        &self,
        matches: &[petgraph::prelude::NodeIndex],
    ) -> HashSet<std::path::PathBuf> {
        let mut unique_paths = HashSet::new();
        let topology = self.index.topology();
        let ref_index = self.index.reference_index();
        let symbols = self.index.symbols();

        for &node_idx in matches {
            let node = &topology[node_idx];

            let (primary, context) = self.extract_smart_tokens(node);

            // Helper to get paths for a token string
            let get_paths = |token: &str| -> Option<Vec<std::path::PathBuf>> {
                let sym = symbols.get(token)?;
                ref_index
                    .get(&naviscope_api::models::symbol::Symbol(sym))
                    .map(|paths| {
                        paths
                            .iter()
                            .map(|p_sym| std::path::PathBuf::from(symbols.resolve(&p_sym.0)))
                            .collect()
                    })
            };

            if let Some(primary_paths) = get_paths(&primary) {
                if let Some(ctx_str) = context {
                    // Optimization: INTERSECTION
                    // Only candidate files that contain BOTH the context (e.g. Class) and name (e.g. Method).
                    if let Some(ctx_paths) = get_paths(&ctx_str) {
                        // SPARSITY CHECK: If context is too generic (e.g. "com", "org", "java"),
                        // intersection is expensive and useless. Skip if it hits > 1000 files.
                        if ctx_paths.len() < 1000 {
                            let ctx_set: HashSet<_> = ctx_paths.iter().collect();
                            for p in primary_paths {
                                if ctx_set.contains(&p) {
                                    unique_paths.insert(p.clone());
                                }
                            }
                            continue; // Optimization applied, skip fallback
                        }
                    }
                }

                // Fallback: Add all files containing the primary token
                for p in primary_paths {
                    unique_paths.insert(p);
                }
            }
        }
        unique_paths
    }

    /// Smartly extract tokens for "bag of words" intersection.
    /// Returns (Primary Token, Optional Context Token)
    fn extract_smart_tokens(&self, node: &crate::model::GraphNode) -> (String, Option<String>) {
        let symbols = self.index.symbols();
        let name = node.name(symbols).to_string();
        let convention = self.get_convention(node);
        let fqn = self.index.render_fqn(node, convention);

        // Split by ANY non-alphanumeric character (except underscore)
        // This is much more language-agnostic than hardcoding '.', ':', etc.
        let parts: Vec<&str> = fqn
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s: &&str| !s.is_empty())
            .collect();

        // Context is usually the immediate parent of the name in the FQN.
        // e.g. "com.example.UserService.login" -> context is "UserService"
        // Also check if last part matches name to ensure alignment
        let context = if parts.len() >= 2 {
            // Check if last part is indeed the name
            if parts.last() == Some(&name.as_str()) {
                Some(parts[parts.len() - 2].to_string())
            } else {
                // Should not happen for valid FQNs usually, but fallback
                None
            }
        } else {
            None
        };

        (name, context)
    }

    /// Micro-level: Scan a specific file for precise symbol occurrences.
    /// Now performs SEMANTIC VERIFICATION using the Resolver.
    pub fn scan_file(
        &self,
        semantic: &dyn SemanticCap,
        source: &str,
        target_resolution: &SymbolResolution,
        uri: &Uri,
    ) -> Vec<Location> {
        if let Some(tree) = semantic.parse(source, None) {
            // 1. Syntactic Scan (Fast)
            let candidates = semantic.find_occurrences(
                source,
                &tree,
                target_resolution,
                Some(self.index.as_plugin_graph()),
            );

            // 2. Semantic Verification (Precise)
            let mut valid_locations = Vec::new();

            for range in candidates {
                // Resolve what is truly at this location
                if let Some(resolved_at_loc) = semantic.resolve_at(
                    &tree,
                    source,
                    range.start_line,
                    range.start_col,
                    self.index.as_plugin_graph(),
                ) {
                    // 3. Identity & inheritance check
                    if semantic.is_reference_to(
                        self.index.as_plugin_graph(),
                        &resolved_at_loc,
                        target_resolution,
                    ) {
                        valid_locations.push(Location {
                            uri: uri.clone(),
                            range: lsp_types::Range {
                                start: lsp_types::Position::new(
                                    range.start_line as u32,
                                    range.start_col as u32,
                                ),
                                end: lsp_types::Position::new(
                                    range.end_line as u32,
                                    range.end_col as u32,
                                ),
                            },
                        });
                    }
                }
            }
            valid_locations
        } else {
            Vec::new()
        }
    }
}
