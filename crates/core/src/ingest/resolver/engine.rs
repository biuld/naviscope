use crate::error::Result;
use crate::ingest::resolver::StubbingManager;
use crate::ingest::scanner::ParsedFile;
use crate::model::source::Language;
use crate::model::{GraphOp, ResolvedUnit};
use naviscope_plugin::{
    BuildCaps, LanguageCaps, NodeMetadataCodec, NodePresenter, ProjectContext, SemanticCap,
};
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;

/// Main resolver that dispatches to specific capabilities based on file path.
pub struct IndexResolver {
    build_caps: Vec<BuildCaps>,
    lang_caps: Vec<LanguageCaps>,
    stubbing: Option<StubbingManager>,
}

impl IndexResolver {
    pub fn new() -> Self {
        Self {
            build_caps: Vec::new(),
            lang_caps: Vec::new(),
            stubbing: None,
        }
    }

    pub fn with_caps(build_caps: Vec<BuildCaps>, lang_caps: Vec<LanguageCaps>) -> Self {
        Self {
            build_caps,
            lang_caps,
            stubbing: None,
        }
    }

    pub fn with_stubbing(mut self, stubbing: StubbingManager) -> Self {
        self.stubbing = Some(stubbing);
        self
    }

    pub fn register_language(&mut self, caps: LanguageCaps) {
        self.lang_caps.push(caps);
    }

    pub fn register_build_tool(&mut self, caps: BuildCaps) {
        self.build_caps.push(caps);
    }

    pub fn get_semantic_cap(&self, language: Language) -> Option<Arc<dyn SemanticCap>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .map(|c| c.semantic.clone())
    }

    pub fn get_node_presenter(&self, language: Language) -> Option<Arc<dyn NodePresenter>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .and_then(|c| c.presentation.node_presenter())
            .or_else(|| {
                self.build_caps
                    .iter()
                    .find(|c| c.build_tool.as_str() == language.as_str())
                    .and_then(|c| c.presentation.node_presenter())
            })
    }

    pub fn get_metadata_codec(&self, language: Language) -> Option<Arc<dyn NodeMetadataCodec>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .and_then(|c| c.metadata_codec.metadata_codec())
            .or_else(|| {
                self.build_caps
                    .iter()
                    .find(|c| c.build_tool.as_str() == language.as_str())
                    .and_then(|c| c.metadata_codec.metadata_codec())
            })
    }

    pub fn get_language_for_path(&self, path: &Path) -> Option<Language> {
        self.lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(path))
            .map(|c| c.language.clone())
    }

    pub fn get_naming_convention(
        &self,
        language: Language,
    ) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        self.lang_caps
            .iter()
            .find(|c| c.language == language)
            .and_then(|c| c.presentation.naming_convention())
    }

    /// Resolve all parsed files into graph operations using a two-phase process.
    /// Returns both the operations and the filled ProjectContext.
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<(Vec<GraphOp>, ProjectContext)> {
        let (mut all_ops, build_files, source_files) = self.prepare_and_partition(files);

        // Phase 1: Build Tools
        let mut project_context = ProjectContext::new();
        let build_ops = self.resolve_build_batch(&build_files, &mut project_context)?;
        all_ops.extend(build_ops);

        // Phase 2: Source Files
        let source_ops = self.resolve_source_batch(&source_files, &project_context)?;
        all_ops.extend(source_ops);

        Ok((all_ops, project_context))
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

    pub fn resolve_stubs(
        &self,
        ops: &[GraphOp],
        routes: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> Vec<crate::ingest::resolver::StubRequest> {
        use crate::ingest::resolver::StubRequest;
        use naviscope_api::models::graph::NodeSource;
        use std::collections::HashSet;

        let mut requests = Vec::new();
        let mut seen_fqns = HashSet::new();

        // 1. Identify all unique external FQNs referenced in the operations
        for op in ops {
            match op {
                GraphOp::AddEdge { to_id, .. } => {
                    let fqn = to_id.to_string();
                    seen_fqns.insert(fqn);
                }
                GraphOp::AddNode {
                    data: Some(node_data),
                } => {
                    if node_data.source == NodeSource::External {
                        seen_fqns.insert(node_data.id.to_string());
                    }
                }
                _ => {}
            }
        }

        if seen_fqns.is_empty() || routes.is_empty() {
            return requests;
        }

        // 2. Schedule each FQN for background resolution
        for fqn in seen_fqns {
            // We only schedule if we have a route for it
            if let Some(paths) = self.find_asset_for_fqn(&fqn, routes) {
                requests.push(StubRequest {
                    fqn,
                    candidate_paths: paths.clone(),
                });
            }
        }
        requests
    }

    /// Schedule stubs using internal manager (for tests/backward compat)
    pub fn schedule_stubs(
        &self,
        ops: &[GraphOp],
        routes: &std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) {
        if let Some(stubbing) = &self.stubbing {
            for req in self.resolve_stubs(ops, routes) {
                stubbing.send(req);
            }
        }
    }

    fn find_asset_for_fqn<'a>(
        &self,
        fqn: &str,
        routes: &'a std::collections::HashMap<String, Vec<std::path::PathBuf>>,
    ) -> Option<&'a Vec<std::path::PathBuf>> {
        // Longest prefix match
        let mut current = fqn.to_string();
        while !current.is_empty() {
            if let Some(paths) = routes.get(&current) {
                return Some(paths);
            }
            if let Some(idx) = current.rfind('.') {
                current.truncate(idx);
            } else {
                break;
            }
        }
        None
    }

    pub fn resolve_build_batch(
        &self,
        build_files: &[ParsedFile],
        context: &mut ProjectContext,
    ) -> Result<Vec<GraphOp>> {
        let mut all_ops = Vec::new();
        for caps in &self.build_caps {
            let tool_files: Vec<&ParsedFile> = build_files
                .iter()
                .filter(|f| caps.matcher.supports_path(f.path()))
                .collect();

            if !tool_files.is_empty() {
                let (unit, ctx) = caps
                    .indexing
                    .compile_build(&tool_files)
                    .map_err(crate::error::NaviscopeError::from)?;
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
                let caps = self
                    .lang_caps
                    .iter()
                    .find(|c| c.matcher.supports_path(file.path()));

                if let Some(c) = caps {
                    c.indexing
                        .compile_source(file, context)
                        .map_err(crate::error::NaviscopeError::from)
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
        let files =
            crate::ingest::scanner::Scanner::scan_files(paths, &std::collections::HashMap::new());
        let (mut all_ops, _build, source) = self.prepare_and_partition(files);

        let source_ops = self.resolve_source_batch(&source, context)?;
        all_ops.extend(source_ops);

        Ok(all_ops)
    }
}
