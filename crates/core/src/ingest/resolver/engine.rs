use crate::error::Result;
use crate::ingest::resolver::StubbingManager;
use crate::ingest::resolver::{ProjectContext, SemanticResolver};
use crate::ingest::scanner::ParsedFile;
use crate::model::source::Language;
use crate::model::{GraphOp, ResolvedUnit};
use rayon::prelude::*;
use std::sync::Arc;

use crate::plugin::{BuildToolPlugin, LanguagePlugin};

/// Main resolver that dispatches to specific strategies based on file type for indexing
pub struct IndexResolver {
    build_plugins: Vec<Arc<dyn BuildToolPlugin>>,
    lang_plugins: Vec<Arc<dyn LanguagePlugin>>,
    stubbing: Option<StubbingManager>,
}

impl IndexResolver {
    pub fn new() -> Self {
        Self {
            build_plugins: Vec::new(),
            lang_plugins: Vec::new(),
            stubbing: None,
        }
    }

    pub fn with_plugins(
        build_plugins: Vec<Arc<dyn BuildToolPlugin>>,
        lang_plugins: Vec<Arc<dyn LanguagePlugin>>,
    ) -> Self {
        Self {
            build_plugins,
            lang_plugins,
            stubbing: None,
        }
    }

    pub fn with_stubbing(mut self, stubbing: StubbingManager) -> Self {
        self.stubbing = Some(stubbing);
        self
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
    ) -> Option<Arc<dyn naviscope_plugin::LspParser>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .map(|p| p.lsp_parser())
    }

    pub fn get_node_adapter(
        &self,
        language: Language,
    ) -> Option<Arc<dyn crate::plugin::NodeAdapter>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .and_then(|p| p.get_node_adapter())
            .or_else(|| {
                self.build_plugins
                    .iter()
                    .find(|p| p.name().as_str() == language.as_str())
                    .and_then(|p| p.get_node_adapter())
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

    pub fn get_naming_convention(
        &self,
        language: Language,
    ) -> Option<Arc<dyn naviscope_plugin::NamingConvention>> {
        self.lang_plugins
            .iter()
            .find(|p| p.name() == language)
            .and_then(|p| p.get_naming_convention())
    }

    /// Resolve all parsed files into graph operations using a two-phase process.
    /// Returns both the operations and the filled ProjectContext (containing asset_routes).
    pub fn resolve(&self, files: Vec<ParsedFile>) -> Result<(Vec<GraphOp>, ProjectContext)> {
        let (mut all_ops, build_files, source_files) = self.prepare_and_partition(files);

        // Phase 1: Build Tools
        let mut project_context = ProjectContext::new();
        let build_ops = self.resolve_build_batch(&build_files, &mut project_context)?;
        all_ops.extend(build_ops);

        // Phase 1.5: Asset Routing (Classpath)
        let asset_ops = self.resolve_assets_batch(&mut project_context)?;
        all_ops.extend(asset_ops);

        // Phase 2: Source Files
        let source_ops = self.resolve_source_batch(&source_files, &project_context)?;

        // Apply asset routes from source ops to context so they describe the full state
        for op in &source_ops {
            if let GraphOp::UpdateAssetRoutes { routes } = op {
                for (prefix, paths) in routes {
                    project_context
                        .asset_routes
                        .entry(prefix.clone())
                        .or_default()
                        .extend(paths.clone());
                }
            }
        }

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

    pub fn resolve_assets_batch(&self, context: &mut ProjectContext) -> Result<Vec<GraphOp>> {
        // 1. Collect and deduplicate all assets
        let mut all_assets = context.builtin_assets.clone();
        all_assets.extend(context.external_assets.clone());
        all_assets.sort();
        all_assets.dedup();

        if all_assets.is_empty() {
            return Ok(vec![]);
        }

        // 2. Index each asset using appropriate language plugins
        for asset in all_assets {
            let ext = asset
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();

            for plugin in &self.lang_plugins {
                let file_name = asset.file_name().and_then(|n| n.to_str()).unwrap_or("");
                // Heuristic: Java plugin handles .jar, .jmod and the 'modules' image
                let is_java_asset = (ext == "jar" || ext == "jmod" || file_name == "modules")
                    && plugin.name().as_str() == "java";
                let is_supported_ext = plugin.supported_extensions().contains(&ext.as_str());

                if is_java_asset || is_supported_ext {
                    if let Some(external) = plugin.external_resolver() {
                        if let Ok(prefixes) = external.index_asset(&asset) {
                            for prefix in prefixes {
                                context
                                    .asset_routes
                                    .entry(prefix)
                                    .or_default()
                                    .push(asset.clone());
                            }
                        }
                    }
                }
            }
        }

        if context.asset_routes.is_empty() {
            Ok(vec![])
        } else {
            Ok(vec![GraphOp::UpdateAssetRoutes {
                routes: context.asset_routes.clone(),
            }])
        }
    }

    pub fn resolve_stubs(
        &self,
        ops: &[GraphOp],
        context: Arc<ProjectContext>,
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

        if seen_fqns.is_empty() || context.asset_routes.is_empty() {
            return requests;
        }

        // 2. Schedule each FQN for background resolution
        for fqn in seen_fqns {
            // We only schedule if we have a route for it
            if let Some(paths) = self.find_asset_for_fqn(&fqn, &context) {
                requests.push(StubRequest {
                    fqn,
                    candidate_paths: paths.clone(),
                });
            }
        }
        requests
    }

    /// Schedule stubs using internal manager (for tests/backward compat)
    pub fn schedule_stubs(&self, ops: &[GraphOp], context: Arc<ProjectContext>) {
        if let Some(stubbing) = &self.stubbing {
            for req in self.resolve_stubs(ops, context) {
                stubbing.send(req);
            }
        }
    }

    fn find_asset_for_fqn<'a>(
        &self,
        fqn: &str,
        context: &'a ProjectContext,
    ) -> Option<&'a Vec<std::path::PathBuf>> {
        // Longest prefix match
        let mut current = fqn.to_string();
        while !current.is_empty() {
            if let Some(paths) = context.asset_routes.get(&current) {
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
                let (unit, ctx) = resolver
                    .resolve(&tool_files)
                    .map_err(crate::error::NaviscopeError::from)?;
                all_ops.extend(unit.ops);
                context.path_to_module.extend(ctx.path_to_module);
                context.external_assets.extend(ctx.external_assets);
                context.builtin_assets.extend(ctx.builtin_assets);
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
                    resolver
                        .resolve(file, context)
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

#[cfg(test)]
mod tests {
    use super::*;
    use naviscope_api::models::Language;
    use naviscope_plugin::{ExternalResolver, GlobalParseResult, LangResolver, LspParser};
    use std::path::{Path, PathBuf};

    struct MockExternalResolver;
    impl ExternalResolver for MockExternalResolver {
        fn index_asset(
            &self,
            asset: &Path,
        ) -> std::result::Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
            if asset.to_str().unwrap().contains("example.jar") {
                Ok(vec!["com.example".to_string()])
            } else {
                Ok(vec![])
            }
        }
        fn generate_stub(
            &self,
            _fqn: &str,
            _asset: &Path,
        ) -> std::result::Result<
            naviscope_plugin::model::IndexNode,
            Box<dyn std::error::Error + Send + Sync>,
        > {
            unimplemented!()
        }
        fn resolve_source(
            &self,
            _fqn: &str,
            _source_asset: &Path,
        ) -> std::result::Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>>
        {
            unimplemented!()
        }
    }

    struct MockLanguagePlugin;
    impl crate::plugin::PluginInstance for MockLanguagePlugin {}
    impl crate::plugin::LanguagePlugin for MockLanguagePlugin {
        fn name(&self) -> Language {
            Language::JAVA
        }
        fn supported_extensions(&self) -> &[&str] {
            &["java"]
        }
        fn parse_file(
            &self,
            _source: &str,
            _path: &Path,
        ) -> std::result::Result<GlobalParseResult, Box<dyn std::error::Error + Send + Sync>>
        {
            unimplemented!()
        }
        fn resolver(&self) -> Arc<dyn SemanticResolver> {
            unimplemented!()
        }
        fn lang_resolver(&self) -> Arc<dyn LangResolver> {
            unimplemented!()
        }
        fn lsp_parser(&self) -> Arc<dyn LspParser> {
            unimplemented!()
        }
        fn external_resolver(&self) -> Option<Arc<dyn ExternalResolver>> {
            Some(Arc::new(MockExternalResolver))
        }
    }

    #[test]
    fn test_resolve_assets_batch() {
        let mut resolver = IndexResolver::new();
        resolver.register_language(Arc::new(MockLanguagePlugin));

        let mut context = ProjectContext::new();
        let asset_path = PathBuf::from("/libs/example.jar");
        context.external_assets.push(asset_path.clone());

        let ops = resolver.resolve_assets_batch(&mut context).unwrap();

        assert_eq!(ops.len(), 1);
        if let GraphOp::UpdateAssetRoutes { routes } = &ops[0] {
            assert_eq!(routes.get("com.example"), Some(&vec![asset_path.clone()]));
        } else {
            panic!("Expected UpdateAssetRoutes operation");
        }

        assert_eq!(
            context.asset_routes.get("com.example"),
            Some(&vec![asset_path])
        );
    }
}
