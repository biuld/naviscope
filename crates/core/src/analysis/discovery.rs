use crate::parser::{LspParser, SymbolResolution};
use crate::query::CodeGraphLike;
use lsp_types::{Location, Url};
use std::collections::HashSet;

/// DiscoveryEngine bridges Meso-level graph knowledge with Micro-level file scanning.
pub struct DiscoveryEngine<'a> {
    index: &'a dyn CodeGraphLike,
}

impl<'a> DiscoveryEngine<'a> {
    pub fn new(index: &'a dyn CodeGraphLike) -> Self {
        Self { index }
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
    pub fn scout_references(
        &self,
        matches: &[petgraph::prelude::NodeIndex],
    ) -> HashSet<std::path::PathBuf> {
        let mut unique_paths = HashSet::new();
        let topology = self.index.topology();
        let ref_index = self.index.reference_index();

        for &node_idx in matches {
            let node = &topology[node_idx];

            let (primary, context) = Self::extract_smart_tokens(node);

            if let Some(primary_paths) = ref_index.get(primary.as_str()) {
                if let Some(ctx_str) = context {
                    // Optimization: INTERSECTION
                    // Only candidate files that contain BOTH the context (e.g. Class) and name (e.g. Method).
                    if let Some(ctx_paths) = ref_index.get(ctx_str.as_str()) {
                        // SPARSITY CHECK: If context is too generic (e.g. "com", "org", "java"),
                        // intersection is expensive and useless. Skip if it hits > 1000 files.
                        if ctx_paths.len() < 1000 {
                            let ctx_set: HashSet<_> = ctx_paths.iter().collect();
                            for p in primary_paths {
                                if ctx_set.contains(p) {
                                    unique_paths.insert(p.to_path_buf());
                                }
                            }
                            continue; // Optimization applied, skip fallback
                        }
                    }
                }

                // Fallback: Add all files containing the primary token
                for p in primary_paths {
                    unique_paths.insert(p.to_path_buf());
                }
            }
        }
        unique_paths
    }

    /// Smartly extract tokens for "bag of words" intersection.
    /// Returns (Primary Token, Optional Context Token)
    fn extract_smart_tokens(node: &crate::model::GraphNode) -> (String, Option<String>) {
        let name = node.name().to_string();
        let fqn = node.fqn();

        // Split by ANY non-alphanumeric character (except underscore)
        // This is much more language-agnostic than hardcoding '.', ':', etc.
        let parts: Vec<&str> = fqn
            .split(|c: char| !c.is_alphanumeric() && c != '_')
            .filter(|s: &&str| !s.is_empty())
            .collect();

        // Context is usually the immediate parent of the name in the FQN.
        // e.g. "com.example.UserService.login" -> context is "UserService"
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
        parser: &dyn LspParser,
        resolver: &dyn crate::resolver::SemanticResolver,
        source: &str,
        target_resolution: &SymbolResolution,
        uri: &Url,
    ) -> Vec<Location> {
        if let Some(tree) = parser.parse(source, None) {
            // 1. Syntactic Scan (Fast)
            let candidates = parser.find_occurrences(source, &tree, target_resolution);

            // 2. Semantic Verification (Precise)
            let mut valid_locations = Vec::new();

            for range in candidates {
                // Resolve what is truly at this location
                if let Some(resolved_at_loc) = resolver.resolve_at(
                    &tree,
                    source,
                    range.start_line,
                    range.start_col,
                    self.index,
                ) {
                    // 3. Identity Check
                    if &resolved_at_loc == target_resolution {
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
