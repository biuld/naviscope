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
            .or_else(|| {
                self.build_plugins
                    .iter()
                    .find(|p| p.name().as_str() == language.as_str())
                    .map(|p| p.clone() as Arc<dyn crate::runtime::plugin::MetadataPlugin>)
            })
    }

    pub fn get_node_renderer(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::runtime::plugin::NodeRenderer>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.clone() as Arc<dyn crate::runtime::plugin::NodeRenderer>)
            .or_else(|| {
                self.build_plugins
                    .iter()
                    .find(|p| p.name().as_str() == language.as_str())
                    .map(|p| p.clone() as Arc<dyn crate::runtime::plugin::NodeRenderer>)
            })
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
        let (mut all_ops, build_files, source_files) = self.prepare_and_partition(files);

        // Phase 1: Build Tools
        let mut project_context = ProjectContext::new();
        let build_ops = self.resolve_build_batch(&build_files, &mut project_context)?;
        all_ops.extend(build_ops);

        // Phase 2: Source Files
        let source_ops = self.resolve_source_batch(&source_files, &project_context)?;
        all_ops.extend(source_ops);

        Ok(all_ops)
    }

    fn prepare_and_partition(
        &self,
        files: Vec<ParsedFile>,
    ) -> (Vec<GraphOp>, Vec<ParsedFile>, Vec<ParsedFile>) {
        let mut all_ops = Vec::new();
        for file in &files {
            all_ops.push(GraphOp::RemovePath {
                path: Arc::from(file.file.path.as_path()),
            });
            all_ops.push(GraphOp::UpdateFile {
                metadata: file.file.clone(),
            });
        }

        let (build_files, source_files): (Vec<_>, Vec<_>) =
            files.into_iter().partition(|f| f.is_build());

        (all_ops, build_files, source_files)
    }

    pub fn resolve_build_batch(
        &self,
        build_files: &[ParsedFile],
        context: &mut ProjectContext,
    ) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();
        for plugin in &self.build_plugins {
            let tool_files: Vec<&ParsedFile> = build_files
                .iter()
                .filter(|f| {
                    if let Some(file_name) = f.path().file_name().and_then(|n| n.to_str()) {
                        plugin.recognize(file_name)
                    } else {
                        false
                    }
                })
                .collect();

            if !tool_files.is_empty() {
                let resolver = plugin.build_resolver();
                let (unit, ctx) = resolver.resolve(&tool_files)?;
                all_ops.extend(unit.ops);
                context.path_to_module.extend(ctx.path_to_module);
            }
        }
        Ok(all_ops)
    }

    pub fn resolve_source_batch(
        &self,
        source_files: &[ParsedFile],
        context: &ProjectContext,
    ) -> Result<Vec<GraphOp>> {
        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .par_iter()
            .map(|file| {
                let language = file.language().unwrap_or(Language::BUILDFILE);
                let plugin = self.lang_plugins.iter().find(|p| p.name() == language);

                if let Some(p) = plugin {
                    let resolver = p.lang_resolver();
                    resolver.resolve(file, context)
                } else {
                    Ok(ResolvedUnit::new())
                }
            })
            .collect();

        let mut all_ops = Vec::new();
        for result in source_results {
            let unit = result?;
            all_ops.extend(unit.ops);
        }
        Ok(all_ops)
    }
}

impl crate::ingest::pipeline::PipelineStage<ProjectContext> for IndexResolver {
    type Output = GraphOp;

    fn process(
        &self,
        context: &ProjectContext,
        paths: Vec<std::path::PathBuf>,
    ) -> Result<Vec<Self::Output>> {
        // In a pipeline batch, we need to scan and then resolve
        // For simplicity in this first step, we assume paths are already filtered
        // We need existing_metadata to avoid redundant parsing, but for a simple pipeline we can skip it or pass it in context
        let files =
            crate::ingest::scanner::Scanner::scan_files(paths, &std::collections::HashMap::new());
        let (mut all_ops, _build, source) = self.prepare_and_partition(files);

        // In this stage, we only care about source files in the pipeline
        let source_ops = self.resolve_source_batch(&source, context)?;
        all_ops.extend(source_ops);

        Ok(all_ops)
    }
}
