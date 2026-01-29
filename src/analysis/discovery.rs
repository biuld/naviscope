use crate::index::CodeGraph;
use crate::model::graph::EdgeType;
use crate::parser::{LspParser, SymbolResolution};
use petgraph::Direction;
use std::collections::HashSet;
use std::path::PathBuf;
use tower_lsp::lsp_types::{Location, Url};

/// DiscoveryEngine bridges Meso-level graph knowledge with Micro-level file scanning.
pub struct DiscoveryEngine<'a> {
    index: &'a CodeGraph,
}

impl<'a> DiscoveryEngine<'a> {
    pub fn new(index: &'a CodeGraph) -> Self {
        Self { index }
    }

    /// Meso-level: Scout for candidate files that likely contain references to the given nodes.
    /// Returns a set of unique file paths.
    pub fn scout_references(&self, matches: &[petgraph::prelude::NodeIndex]) -> HashSet<PathBuf> {
        let mut unique_paths = HashSet::new();

        for &node_idx in matches {
            let mut incoming = self
                .index
                .topology
                .neighbors_directed(node_idx, Direction::Incoming)
                .detach();
            while let Some((edge_idx, neighbor_idx)) = incoming.next(&self.index.topology) {
                let edge = &self.index.topology[edge_idx];

                // Filter edges for references
                match edge.edge_type {
                    EdgeType::Calls
                    | EdgeType::Instantiates
                    | EdgeType::TypedAs
                    | EdgeType::DecoratedBy => {
                        if let Some(source_path) = self.index.topology[neighbor_idx].file_path() {
                            unique_paths.insert(source_path.clone());
                        }
                    }
                    _ => continue,
                }
            }
        }
        unique_paths
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
                    range: tower_lsp::lsp_types::Range {
                        start: tower_lsp::lsp_types::Position::new(
                            r.start_line as u32,
                            r.start_col as u32,
                        ),
                        end: tower_lsp::lsp_types::Position::new(
                            r.end_line as u32,
                            r.end_col as u32,
                        ),
                    },
                })
                .collect()
        } else {
            Vec::new()
        }
    }
}
