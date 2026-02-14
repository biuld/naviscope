use crate::error::Result;
use crate::indexing::scanner::ParsedFile;
use crate::indexing::source_runtime::{self, SourceCompilerRuntime};
use crate::model::GraphOp;
use naviscope_plugin::{BuildCaps, BuildContent, LanguageCaps, ParsedContent, ProjectContext};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

pub struct BatchCompiler {
    build_caps: Vec<BuildCaps>,
    source_compiler_runtime: tokio::sync::OnceCell<Arc<SourceCompilerRuntime>>,
    pending_stub_requests: Mutex<Vec<crate::indexing::StubRequest>>,
}

impl BatchCompiler {
    pub fn with_caps(build_caps: Vec<BuildCaps>) -> Self {
        Self {
            build_caps,
            source_compiler_runtime: tokio::sync::OnceCell::const_new(),
            pending_stub_requests: Mutex::new(Vec::new()),
        }
    }

    pub fn compile_build_batch(
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
                let parsed_tool_files: Vec<ParsedFile> = tool_files
                    .iter()
                    .map(|f| Self::prepare_build_file(caps, f))
                    .collect::<Result<Vec<_>>>()?;
                let parsed_tool_file_refs: Vec<&ParsedFile> = parsed_tool_files.iter().collect();
                let (unit, ctx) = caps
                    .indexing
                    .compile_build(&parsed_tool_file_refs)
                    .map_err(crate::error::NaviscopeError::from)?;
                all_ops.extend(unit.ops);
                context.path_to_module.extend(ctx.path_to_module);
            }
        }
        Ok(all_ops)
    }

    pub async fn start_source_runtime(
        current: Arc<tokio::sync::RwLock<Arc<crate::model::CodeGraph>>>,
        naming_conventions: Arc<
            std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>,
        >,
        build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> Result<SourceCompilerRuntime> {
        SourceCompilerRuntime::start(
            current,
            naming_conventions,
            build_caps,
            lang_caps,
            stub_cache,
        )
        .await
    }

    pub async fn ensure_source_compiler_runtime(
        &self,
        current: Arc<tokio::sync::RwLock<Arc<crate::model::CodeGraph>>>,
        naming_conventions: Arc<
            std::collections::HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>,
        >,
        build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> Result<Arc<SourceCompilerRuntime>> {
        let runtime = self
            .source_compiler_runtime
            .get_or_try_init(|| async {
                Self::start_source_runtime(
                    current,
                    naming_conventions,
                    build_caps,
                    lang_caps,
                    stub_cache,
                )
                .await
                .map(Arc::new)
            })
            .await
            .map(Arc::clone)?;

        let drained = match self.pending_stub_requests.lock() {
            Ok(mut pending) => pending.drain(..).collect::<Vec<_>>(),
            Err(_) => Vec::new(),
        };
        for req in drained {
            if let Err(err) = Self::submit_stub_request(runtime.as_ref(), req).await {
                tracing::warn!("Failed to submit deferred stub request: {}", err);
            }
        }

        Ok(runtime)
    }

    pub async fn compile_source_batch(
        runtime: &SourceCompilerRuntime,
        source_files: Vec<ParsedFile>,
        project_context: ProjectContext,
        routes: std::collections::HashMap<String, Vec<PathBuf>>,
    ) -> Result<()> {
        runtime
            .submit_source_batch(source_files, project_context, routes)
            .await
    }

    pub async fn submit_stub_request(
        runtime: &SourceCompilerRuntime,
        req: crate::indexing::StubRequest,
    ) -> Result<()> {
        runtime.submit_stub_request(req).await
    }

    pub fn try_submit_stub_request(
        runtime: &SourceCompilerRuntime,
        req: crate::indexing::StubRequest,
    ) -> Result<()> {
        runtime.try_submit_stub_request(req)
    }

    pub fn try_submit_or_enqueue_stub_request(&self, req: crate::indexing::StubRequest) -> bool {
        if let Some(runtime) = self.source_compiler_runtime.get() {
            return runtime.try_submit_stub_request(req).is_ok();
        }

        if let Ok(mut pending) = self.pending_stub_requests.lock() {
            pending.push(req);
            return true;
        }

        false
    }

    pub fn plan_stub_requests(
        ops: &[GraphOp],
        routes: &std::collections::HashMap<String, Vec<PathBuf>>,
    ) -> Vec<crate::indexing::StubRequest> {
        source_runtime::plan_stub_requests(ops, routes)
    }

    fn prepare_build_file(caps: &BuildCaps, file: &ParsedFile) -> Result<ParsedFile> {
        let source = match &file.content {
            ParsedContent::Unparsed(s) => s.clone(),
            ParsedContent::Lazy => fs::read_to_string(file.path()).map_err(|e| {
                crate::error::NaviscopeError::Internal(format!(
                    "Failed to read build file {}: {}",
                    file.path().display(),
                    e
                ))
            })?,
            ParsedContent::Metadata(_) => return Ok(file.clone()),
            ParsedContent::Language(_) => return Ok(file.clone()),
        };

        let parse_result = caps
            .parser
            .parse_build_file(&source)
            .map_err(crate::error::NaviscopeError::from)?;

        let content = match parse_result.content {
            BuildContent::Metadata(value) => ParsedContent::Metadata(value),
            BuildContent::Unparsed(text) => ParsedContent::Unparsed(text),
            // Build indexing currently consumes Metadata/Unparsed; preserve source for this case.
            BuildContent::Parsed(_) => ParsedContent::Unparsed(source),
        };

        Ok(ParsedFile {
            file: file.file.clone(),
            content,
        })
    }
}
