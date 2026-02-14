mod commit_sink;
mod deferred_queue;
mod executor;
mod metrics;
mod stub_ops;

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use naviscope_ingest::{
    IngestError, IngestRuntime, IntakeHandle, RuntimeComponents, RuntimeConfig,
};
use naviscope_plugin::{BuildCaps, LanguageCaps, ParsedFile, ProjectContext};

use crate::error::{NaviscopeError, Result};
use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp};

use commit_sink::CommitGraphSink;
use deferred_queue::InMemoryDeferredQueue;
use executor::IngestExecutor;
use metrics::NoopRuntimeMetrics;
pub use stub_ops::plan_stub_requests;

#[derive(Clone)]
pub enum SourceCompileWorkItem {
    SourceCollect(ParsedFile),
    SourceAnalyze(ParsedFile),
    SourceLower(ParsedFile),
    StubRequest(StubRequest),
}

#[derive(Clone)]
struct StagedSourceItem {
    file: ParsedFile,
    collect_id: String,
}

pub struct SourceCompilerRuntime {
    intake: IntakeHandle<SourceCompileWorkItem>,
    project_context: Arc<RwLock<ProjectContext>>,
    routes: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
    runtime_task: tokio::task::JoinHandle<()>,
}

impl SourceCompilerRuntime {
    pub async fn start(
        current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
        naming_conventions: Arc<HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>>,
        _build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> Result<Self> {
        let project_context = Arc::new(RwLock::new(ProjectContext::new()));
        let routes = Arc::new(RwLock::new(HashMap::new()));
        let executor: naviscope_ingest::DynExecutor<SourceCompileWorkItem, GraphOp> =
            Arc::new(IngestExecutor {
                lang_caps,
                project_context: Arc::clone(&project_context),
                routes: Arc::clone(&routes),
                current: Arc::clone(&current),
                stub_cache,
                collect_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
                analyze_cache: Arc::new(std::sync::Mutex::new(HashMap::new())),
            });
        let deferred_store: naviscope_ingest::DynDeferredStore<SourceCompileWorkItem> =
            Arc::new(InMemoryDeferredQueue::default());
        let commit_sink: naviscope_ingest::DynCommitSink<GraphOp> = Arc::new(CommitGraphSink {
            current,
            naming_conventions,
        });
        let metrics: naviscope_ingest::DynRuntimeMetrics = Arc::new(NoopRuntimeMetrics);

        let runtime = Arc::new(IngestRuntime::new(
            RuntimeConfig {
                kernel_channel_capacity: 500,
                max_in_flight: 256,
                deferred_poll_limit: 256,
                idle_sleep_ms: 10,
            },
            RuntimeComponents::with_tokio_bus(
                executor,
                deferred_store,
                commit_sink,
                metrics,
            ),
        ));

        let intake = runtime.intake_handle();
        let runtime_clone = Arc::clone(&runtime);
        let runtime_task = tokio::spawn(async move {
            if let Err(err) = runtime_clone.run_forever().await {
                tracing::warn!("source compiler runtime stopped: {}", err);
            }
        });

        Ok(Self {
            intake,
            project_context,
            routes,
            runtime_task,
        })
    }

    pub async fn submit_source_batch(
        &self,
        source_files: Vec<ParsedFile>,
        project_context: ProjectContext,
        routes: HashMap<String, Vec<PathBuf>>,
    ) -> Result<()> {
        {
            let mut guard = self
                .project_context
                .write()
                .map_err(|_| NaviscopeError::Internal("project context poisoned".to_string()))?;
            *guard = project_context;
        }
        {
            let mut guard = self
                .routes
                .write()
                .map_err(|_| NaviscopeError::Internal("routes map poisoned".to_string()))?;
            *guard = routes;
        }

        if source_files.is_empty() {
            return Ok(());
        }

        let mut staged = Vec::new();
        let epoch = self.intake.new_epoch();
        for (index, file) in source_files.into_iter().enumerate() {
            let base = format!("src:{}:{}", index, file.path().display());
            let collect_id = format!("{base}:collect");
            staged.push(StagedSourceItem {
                file,
                collect_id,
            });
        }

        for item in staged {
            let file = item.file;
            let collect_id = item.collect_id;
            let collect_msg = naviscope_ingest::Message {
                msg_id: collect_id.clone(),
                topic: "source-collect".to_string(),
                message_group: file.path().to_string_lossy().to_string(),
                version: 1,
                depends_on: Vec::new(),
                epoch,
                payload: SourceCompileWorkItem::SourceCollect(file.clone()),
                metadata: BTreeMap::new(),
            };
            self.intake
                .submit(collect_msg)
                .await
                .map_err(ingest_to_naviscope_error)?;
        }

        self.intake
            .seal_epoch(epoch)
            .map_err(ingest_to_naviscope_error)?;
        self.intake
            .wait_epoch(epoch)
            .await
            .map_err(ingest_to_naviscope_error)
    }

    pub async fn submit_stub_request(&self, req: StubRequest) -> Result<()> {
        let msg_id = format!("stub:{}", req.fqn);
        let msg = naviscope_ingest::Message {
            msg_id,
            topic: "stub-index".to_string(),
            message_group: req.fqn.clone(),
            version: 1,
            depends_on: Vec::new(),
            epoch: 0,
            payload: SourceCompileWorkItem::StubRequest(req),
            metadata: BTreeMap::new(),
        };
        self.intake
            .submit(msg)
            .await
            .map_err(ingest_to_naviscope_error)
    }

    pub fn try_submit_stub_request(&self, req: StubRequest) -> Result<()> {
        let msg_id = format!("stub:{}", req.fqn);
        let msg = naviscope_ingest::Message {
            msg_id,
            topic: "stub-index".to_string(),
            message_group: req.fqn.clone(),
            version: 1,
            depends_on: Vec::new(),
            epoch: 0,
            payload: SourceCompileWorkItem::StubRequest(req),
            metadata: BTreeMap::new(),
        };
        self.intake
            .try_submit(msg)
            .map_err(ingest_to_naviscope_error)
    }
}

impl Drop for SourceCompilerRuntime {
    fn drop(&mut self) {
        self.runtime_task.abort();
    }
}

fn ingest_to_naviscope_error(err: IngestError) -> NaviscopeError {
    NaviscopeError::Internal(err.to_string())
}
