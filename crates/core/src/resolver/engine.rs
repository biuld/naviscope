use crate::error::Result;
use crate::model::{GraphOp, ResolvedUnit};
use crate::project::scanner::ParsedFile;
use crate::project::source::Language;
use crate::resolver::{ProjectContext, SemanticResolver};
use rayon::prelude::*;
use std::sync::Arc;

use crate::engine::storage::GLOBAL_POOL;
use crate::plugin::{BuildToolPlugin, LanguagePlugin};

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
        // Find plugin by name or language mapping
        // For now, let's assume Language maps to plugin name lowercase
        let name = match language {
            Language::Java => "java",
            _ => return None,
        };
        self.lang_plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.resolver())
    }

    pub fn get_lsp_parser(&self, language: Language) -> Option<Arc<dyn crate::parser::LspParser>> {
        let name = match language {
            Language::Java => "java",
            _ => return None,
        };
        self.lang_plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.lsp_parser())
    }

    pub fn get_language_by_extension(&self, ext: &str) -> Option<Language> {
        // This is a bit awkward as Language enum is hardcoded but plugins are dynamic.
        // Ideally we should move away from Language enum or make it dynamic.
        // For now, hardcode mapping to plugins.
        for plugin in &self.lang_plugins {
            if plugin.supported_extensions().contains(&ext) {
                // Map plugin name to Language enum
                return match plugin.name() {
                    "java" => Some(Language::Java),
                    _ => None,
                };
            }
        }
        if ext == "gradle" || ext == "kts" {
            return Some(Language::BuildFile);
        }
        None
    }

    pub fn get_feature_provider(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::plugin::LanguageFeatureProvider>> {
        let name = match language {
            Language::Java => "java",
            _ => return None,
        };
        self.lang_plugins
            .iter()
            .find(|p| p.name() == name)
            .map(|p| p.feature_provider())
    }

    /// Resolve all parsed files into graph operations using a two-phase process
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();

        // Add RemovePath operations and UpdateFile operations for each file being processed
        for file in &files {
            all_ops.push(GraphOp::RemovePath {
                path: GLOBAL_POOL.intern_path(&file.file.path),
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

        // We need to group files by build tool plugin
        // But ParsedFile doesn't store plugin reference, only BuildTool enum.
        // We iterate plugins and ask them to recognize files?
        // Or we use the build_tool() method which returns enum, and map enum to plugin name.

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
        // We need to capture plugins in the closure, so we need Arc<Vec> or similar.
        // Since we are inside &self method, we can't easily pass self.lang_plugins to par_iter/map unless they are Sync.
        // LangResolver traits are Sync.

        // However, looking up the right plugin for each file inside par_iter might be slow if we have many plugins.
        // But here we only have few.

        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .par_iter()
            .map(|file| {
                let language = file.language().unwrap_or(Language::BuildFile);
                let name = match language {
                    Language::Java => "java",
                    _ => return Ok(ResolvedUnit::new()),
                };

                // We cannot access self.lang_plugins here easily because self is not Sync/Send via reference in par_iter if we capture it incorrectly?
                // Actually helper method would be better or passing plugins as argument to closure.
                // But we can't pass self fields easily.
                // NOTE: Vec and Arc are Send+Sync. self is &IndexResolver.

                // Let's assume we can access plugins from self if we clone arcs before?
                // Or we can just unsafe it? NO.
                // We are inside `map`, which is executed on Rayon thread.

                // Hack: we cannot iterate self.lang_plugins inside par_iter because `self` might not be safely sharable if we consider mutable methods (unlikely providing &self).
                // But IndexResolver only has &self methods here.
                // The issue is avoiding O(N) lookup.

                // But wait, the closure captures `&self`.

                let plugin = self.lang_plugins.iter().find(|p| p.name() == name);

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
            all_ops.extend(result?.ops);
        }

        Ok(all_ops)
    }
}
