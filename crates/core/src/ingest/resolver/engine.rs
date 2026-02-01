use crate::error::Result;
use crate::ingest::resolver::{ProjectContext, SemanticResolver};
use crate::ingest::scanner::ParsedFile;
use crate::model::source::Language;
use crate::model::{GraphOp, ResolvedUnit};
use rayon::prelude::*;
use std::sync::Arc;

use crate::runtime::plugin::{BuildToolPlugin, LanguagePlugin};

/// Main resolver that dispatches to specific strategies based on file type for indexing
pub struct IndexResolver {
    build_plugins: Vec<Arc<dyn BuildToolPlugin>>,
    lang_plugins: Vec<Arc<dyn LanguagePlugin>>,
}

impl IndexResolver {
    pub fn new() -> Self {
        Self {
            build_plugins: Vec::new(),
            lang_plugins: Vec::new(),
        }
    }

    pub fn with_plugins(
        build_plugins: Vec<Arc<dyn BuildToolPlugin>>,
        lang_plugins: Vec<Arc<dyn LanguagePlugin>>,
    ) -> Self {
        Self {
            build_plugins,
            lang_plugins,
        }
    }

    pub fn register_language(&mut self, plugin: Arc<dyn LanguagePlugin>) {
        self.lang_plugins.push(plugin);
    }

    pub fn register_build_tool(&mut self, plugin: Arc<dyn BuildToolPlugin>) {
        self.build_plugins.push(plugin);
    }

    pub fn get_semantic_resolver(&self, language: Language) -> Option<Arc<dyn SemanticResolver>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.resolver())
    }

    pub fn get_lsp_parser(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::ingest::parser::LspParser>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.lsp_parser())
    }

    pub fn get_metadata_plugin(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::runtime::plugin::MetadataPlugin>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.clone() as Arc<dyn crate::runtime::plugin::MetadataPlugin>)
    }

    pub fn get_node_renderer(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::runtime::plugin::NodeRenderer>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.clone() as Arc<dyn crate::runtime::plugin::NodeRenderer>)
    }

    pub fn get_language_by_extension(&self, ext: &str) -> Option<Language> {
        for plugin in &self.lang_plugins {
            if plugin.supported_extensions().contains(&ext) {
                return Some(plugin.name());
            }
        }
        Language::from_extension(ext)
    }

    /// Resolve all parsed files into graph operations using a two-phase process
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();

        // Add RemovePath operations and UpdateFile operations for each file being processed
        for file in &files {
            all_ops.push(GraphOp::RemovePath {
                path: Arc::from(file.file.path.as_path()),
            });
            all_ops.push(GraphOp::UpdateFile {
                metadata: file.file.clone(),
            });
        }

        // Separate files into build and source files
        let (build_files, source_files): (Vec<_>, Vec<_>) =
            files.into_iter().partition(|f| f.is_build());

        // Phase 1: Resolve Build Tools (Structure)
        let mut project_context = ProjectContext::new();

        for plugin in &self.build_plugins {
            // Find files relevant to this plugin
            let tool_files: Vec<ParsedFile> = build_files
                .iter()
                .filter(|f| {
                    if let Some(file_name) = f.path().file_name().and_then(|n| n.to_str()) {
                        plugin.recognize(file_name)
                    } else {
                        false
                    }
                })
                .cloned()
                .collect();

            let tool_files_refs: Vec<&ParsedFile> = tool_files.iter().collect();

            if !tool_files.is_empty() {
                let resolver = plugin.build_resolver();
                let (unit, context) = resolver.resolve(&tool_files_refs)?;
                all_ops.extend(unit.ops);
                project_context
                    .path_to_module
                    .extend(context.path_to_module);
            }
        }

        // Phase 2: Resolve Source Files (Entities) in parallel
        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .par_iter()
            .map(|file| {
                let language = file.language().unwrap_or(Language::BUILDFILE);
                let plugin = self.lang_plugins.iter().find(|p| p.name() == language);

                if let Some(p) = plugin {
                    let resolver = p.lang_resolver();
                    resolver.resolve(file, &project_context)
                } else {
                    Ok(ResolvedUnit::new())
                }
            })
            .collect();

        // Collect and merge source operations
        for result in source_results {
            let unit = result?;
            all_ops.extend(unit.ops);
        }

        Ok(all_ops)
    }
}
