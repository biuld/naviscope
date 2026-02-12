use naviscope_api::models::SymbolResolution;
use naviscope_core::model::CodeGraph;
use naviscope_java::JavaPlugin;
use naviscope_plugin::LspSyntaxService;
use std::path::PathBuf;
use tree_sitter::Tree;

pub fn collect_occurrence_counts_by_file(
    resolver: &JavaPlugin,
    index: &CodeGraph,
    trees: &[(PathBuf, String, Tree)],
    target: &SymbolResolution,
) -> Vec<(String, usize)> {
    trees
        .iter()
        .map(|(path, source, tree)| {
            let ranges = resolver.find_occurrences(source, tree, target, Some(index));
            (path.to_string_lossy().to_string(), ranges.len())
        })
        .collect()
}
