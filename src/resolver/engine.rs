use crate::error::Result;
use crate::model::graph::{GraphOp, ResolvedUnit};
use crate::project::scanner::ParsedFile;
use crate::project::source::{BuildTool, Language};
use crate::resolver::lang::gradle::GradleResolver;
use crate::resolver::lang::java::JavaResolver;
use crate::resolver::{BuildResolver, LangResolver, ProjectContext, SemanticResolver};
use rayon::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Main resolver that dispatches to specific strategies based on file type for indexing
pub struct IndexResolver {
    build_strategies: HashMap<BuildTool, Box<dyn BuildResolver>>,
    lang_strategies: HashMap<Language, Box<dyn LangResolver>>,
    semantic_resolvers: HashMap<Language, Box<dyn SemanticResolver>>,
}

impl IndexResolver {
    pub fn new() -> Self {
        let mut build_strategies: HashMap<BuildTool, Box<dyn BuildResolver>> = HashMap::new();
        let mut lang_strategies: HashMap<Language, Box<dyn LangResolver>> = HashMap::new();
        let mut semantic_resolvers: HashMap<Language, Box<dyn SemanticResolver>> = HashMap::new();

        // Register build strategies
        build_strategies.insert(BuildTool::Gradle, Box::new(GradleResolver::new()));

        // Register language strategies
        let java_resolver = JavaResolver::new();
        lang_strategies.insert(Language::Java, Box::new(java_resolver.clone()));
        semantic_resolvers.insert(Language::Java, Box::new(java_resolver));

        Self {
            build_strategies,
            lang_strategies,
            semantic_resolvers,
        }
    }

    pub fn get_semantic_resolver(&self, language: Language) -> Option<&dyn SemanticResolver> {
        self.semantic_resolvers.get(&language).map(|r| r.as_ref())
    }

    pub fn get_lsp_parser(&self, language: Language) -> Option<Arc<dyn crate::parser::LspParser>> {
        match language {
            Language::Java => Some(Arc::new(crate::parser::java::JavaParser::new().ok()?)),
            _ => None,
        }
    }

    pub fn get_language_by_extension(&self, ext: &str) -> Option<Language> {
        match ext {
            "java" => Some(Language::Java),
            "gradle" => Some(Language::BuildFile),
            _ => None,
        }
    }

    /// Resolve all parsed files into graph operations using a two-phase process
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();

        // Add RemovePath operations and UpdateFile operations for each file being processed
        for file in &files {
            all_ops.push(GraphOp::RemovePath {
                path: file.file.path.clone(),
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

        // Group build files by tool
        let mut builds_by_tool: HashMap<BuildTool, Vec<&ParsedFile>> = HashMap::new();
        for f in &build_files {
            if let Some(tool) = f.build_tool() {
                builds_by_tool.entry(tool).or_default().push(f);
            }
        }

        for (tool, tool_files) in builds_by_tool {
            if let Some(strategy) = self.build_strategies.get(&tool) {
                let (unit, context) = strategy.resolve(&tool_files)?;
                all_ops.extend(unit.ops);
                // Merge context
                project_context
                    .path_to_module
                    .extend(context.path_to_module);
            }
        }

        // Phase 2: Resolve Source Files (Entities) in parallel
        let source_results: Vec<Result<ResolvedUnit>> = source_files
            .par_iter()
            .map(|file| {
                let language = file.language().unwrap_or(Language::BuildFile);

                if let Some(strategy) = self.lang_strategies.get(&language) {
                    strategy.resolve(file, &project_context)
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
