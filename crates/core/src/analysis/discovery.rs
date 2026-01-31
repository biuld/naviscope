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
    pub fn scout_references(
        &self,
        matches: &[petgraph::prelude::NodeIndex],
    ) -> HashSet<std::path::PathBuf> {
        let mut unique_paths = HashSet::new();
        let topology = self.index.topology();
        let ref_index = self.index.reference_index();

        for &node_idx in matches {
            let node = &topology[node_idx];

            // 1. Reference Index "Scouting" - Extract all identifier tokens from FQN
            // For a node like "com.example.UserService.login", we want to search for:
            // - "login" (method name)
            // - "UserService" (class name)
            // - "example" (package name segment, optional)
            let tokens_to_search = Self::extract_identifier_tokens(node);

            for token in tokens_to_search {
                if let Some(paths) = ref_index.get(token.as_str()) {
                    for p in paths {
                        unique_paths.insert(p.to_path_buf());
                    }
                }
            }
        }
        unique_paths
    }

    /// Extract all possible identifier tokens from a node's FQN and name.
    /// This helps maximize the effectiveness of reference_index lookup.
    fn extract_identifier_tokens(node: &crate::model::graph::GraphNode) -> Vec<String> {
        let mut tokens = Vec::new();

        // Always include the node's simple name (e.g., "login" for a method)
        tokens.push(node.name().to_string());

        // Extract tokens from FQN (e.g., "com.example.UserService.login")
        let fqn = node.fqn();

        // Split by common separators: '.', '::', '#'
        // For Java: "com.example.UserService.login" -> ["com", "example", "UserService", "login"]
        // For modules: "module::root" -> ["module", "root"]
        let parts: Vec<&str> = fqn
            .split(|c| c == '.' || c == '#' || c == ':')
            .filter(|s| !s.is_empty())
            .collect();

        // Add all parts as potential tokens (but skip duplicates)
        for part in parts {
            let part_str = part.to_string();
            if !tokens.contains(&part_str) {
                tokens.push(part_str);
            }
        }

        tokens
    }

    /// Micro-level: Scan a specific file for precise symbol occurrences.
    pub fn scan_file(
        &self,
        parser: &dyn LspParser,
        source: &str,
        resolution: &SymbolResolution,
        uri: &Url,
    ) -> Vec<Location> {
        if let Some(tree) = parser.parse(source, None) {
            let ranges = parser.find_occurrences(source, &tree, resolution);
            ranges
                .into_iter()
                .map(|r| Location {
                    uri: uri.clone(),
                    range: lsp_types::Range {
                        start: lsp_types::Position::new(r.start_line as u32, r.start_col as u32),
                        end: lsp_types::Position::new(r.end_line as u32, r.end_col as u32),
                    },
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}
