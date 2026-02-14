mod batch_tracker;
mod workers;

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

use batch_tracker::BatchTracker;
use workers::{
    CommitGraphSink, InMemoryDeferredQueue, IngestExecutor, NoopRuntimeMetrics,
    PassThroughScheduler,
};

#[derive(Clone)]
pub enum IngestWorkItem {
    SourceFile(ParsedFile),
    StubRequest(StubRequest),
}

pub struct IngestAdapter {
    intake: IntakeHandle<IngestWorkItem>,
    project_context: Arc<RwLock<ProjectContext>>,
    routes: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
    batch_tracker: Arc<BatchTracker>,
    runtime_task: tokio::task::JoinHandle<()>,
}

impl IngestAdapter {
    pub async fn start(
        current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
        naming_conventions: Arc<HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>>,
        build_caps: Arc<Vec<BuildCaps>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> Result<Self> {
        let project_context = Arc::new(RwLock::new(ProjectContext::new()));
        let routes = Arc::new(RwLock::new(HashMap::new()));
        let batch_tracker = Arc::new(BatchTracker::default());

        let scheduler: naviscope_ingest::DynScheduler<IngestWorkItem, GraphOp> =
            Arc::new(PassThroughScheduler);
        let executor: naviscope_ingest::DynExecutor<IngestWorkItem, GraphOp> =
            Arc::new(IngestExecutor {
                build_caps,
                lang_caps,
                project_context: Arc::clone(&project_context),
                routes: Arc::clone(&routes),
                current: Arc::clone(&current),
                stub_cache,
            });
        let deferred_store: naviscope_ingest::DynDeferredStore<IngestWorkItem> =
            Arc::new(InMemoryDeferredQueue::default());
        let commit_sink: naviscope_ingest::DynCommitSink<GraphOp> = Arc::new(CommitGraphSink {
            current,
            naming_conventions,
            batch_tracker: Arc::clone(&batch_tracker),
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
                scheduler,
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
                tracing::warn!("ingest runtime stopped: {}", err);
            }
        });

        Ok(Self {
            intake,
            project_context,
            routes,
            batch_tracker,
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

        let msg_ids: Vec<String> = source_files
            .iter()
            .enumerate()
            .map(|(index, file)| format!("src:{}:{}", index, file.path().display()))
            .collect();

        let (batch_id, done_rx) = self.batch_tracker.register_batch(&msg_ids);

        for (index, file) in source_files.into_iter().enumerate() {
            let msg = naviscope_ingest::Message {
                msg_id: msg_ids[index].clone(),
                topic: "source-index".to_string(),
                message_group: file.path().to_string_lossy().to_string(),
                version: 1,
                depends_on: Vec::new(),
                epoch: batch_id,
                payload: IngestWorkItem::SourceFile(file),
                metadata: BTreeMap::new(),
            };
            self.intake
                .submit(msg)
                .await
                .map_err(ingest_to_naviscope_error)?;
        }

        done_rx
            .await
            .map_err(|_| NaviscopeError::Internal("ingest batch completion dropped".to_string()))
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
            payload: IngestWorkItem::StubRequest(req),
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
            payload: IngestWorkItem::StubRequest(req),
            metadata: BTreeMap::new(),
        };
        self.intake
            .try_submit(msg)
            .map_err(ingest_to_naviscope_error)
    }
}

impl Drop for IngestAdapter {
    fn drop(&mut self) {
        self.runtime_task.abort();
    }
}

fn ingest_to_naviscope_error(err: IngestError) -> NaviscopeError {
    NaviscopeError::Internal(err.to_string())
}
