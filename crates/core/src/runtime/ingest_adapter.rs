use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use naviscope_ingest::runtime::kernel;
use naviscope_ingest::{
    CommitSink, DeferredStore, DependencyReadyEvent, ExecutionResult, ExecutionStatus,
    FlowControlConfig, IngestError, PipelineBus, PipelineEvent, RuntimeConfig, RuntimeMetrics,
    Scheduler,
};
use naviscope_plugin::{LanguageCaps, ParsedFile, ProjectContext};

use crate::error::{NaviscopeError, Result};
use crate::ingest::resolver::{IndexResolver, StubRequest};
use crate::model::{CodeGraph, GraphOp};

#[derive(Clone)]
pub struct IndexWorkItem {
    pub file: ParsedFile,
}

struct IndexWorkScheduler;

impl Scheduler<IndexWorkItem, GraphOp> for IndexWorkScheduler {
    fn schedule(
        &self,
        messages: Vec<naviscope_ingest::Message<IndexWorkItem>>,
    ) -> std::result::Result<Vec<PipelineEvent<IndexWorkItem, GraphOp>>, IngestError> {
        Ok(messages.into_iter().map(PipelineEvent::Runnable).collect())
    }
}

struct IndexWorkExecutor {
    resolver: Arc<IndexResolver>,
    project_context: Arc<ProjectContext>,
}

impl naviscope_ingest::Executor<IndexWorkItem, GraphOp> for IndexWorkExecutor {
    fn execute(
        &self,
        message: naviscope_ingest::Message<IndexWorkItem>,
    ) -> std::result::Result<Vec<PipelineEvent<IndexWorkItem, GraphOp>>, IngestError> {
        let file = message.payload.file;
        let path = file.path().to_path_buf();

        let mut ops = Vec::with_capacity(4);
        ops.push(GraphOp::RemovePath {
            path: Arc::from(path.as_path()),
        });
        ops.push(GraphOp::UpdateFile {
            metadata: file.file.clone(),
        });

        let source_ops = self
            .resolver
            .resolve_source_batch(std::slice::from_ref(&file), &self.project_context)
            .map_err(naviscope_to_ingest_error)?;
        ops.extend(source_ops);

        Ok(vec![PipelineEvent::Executed {
            epoch: message.epoch,
            result: ExecutionResult {
                msg_id: message.msg_id,
                status: ExecutionStatus::Done,
                operations: ops,
                next_dependencies: Vec::new(),
                error: None,
            },
        }])
    }
}

#[derive(Default)]
struct InMemoryDeferredQueue {
    items: Mutex<VecDeque<naviscope_ingest::Message<IndexWorkItem>>>,
}

impl DeferredStore<IndexWorkItem> for InMemoryDeferredQueue {
    fn push(
        &self,
        message: naviscope_ingest::Message<IndexWorkItem>,
    ) -> std::result::Result<(), IngestError> {
        let mut guard = self
            .items
            .lock()
            .map_err(|_| IngestError::Execution("deferred queue poisoned".to_string()))?;
        guard.push_back(message);
        Ok(())
    }

    fn pop_ready(
        &self,
        limit: usize,
    ) -> std::result::Result<Vec<naviscope_ingest::Message<IndexWorkItem>>, IngestError> {
        let mut guard = self
            .items
            .lock()
            .map_err(|_| IngestError::Execution("deferred queue poisoned".to_string()))?;
        let mut out = Vec::new();
        let max = limit.max(1);
        for _ in 0..max {
            if let Some(item) = guard.pop_front() {
                out.push(item);
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn notify_ready(&self, _event: DependencyReadyEvent) -> std::result::Result<(), IngestError> {
        Ok(())
    }
}

struct IndexWorkCommitter {
    builder: Arc<Mutex<crate::ingest::builder::CodeGraphBuilder>>,
    pending_stubs: Arc<Mutex<Vec<StubRequest>>>,
    resolver: Arc<IndexResolver>,
    routes: Arc<HashMap<String, Vec<PathBuf>>>,
}

impl CommitSink<GraphOp> for IndexWorkCommitter {
    fn commit_epoch(
        &self,
        _epoch: u64,
        results: Vec<ExecutionResult<GraphOp>>,
    ) -> std::result::Result<usize, IngestError> {
        if results.is_empty() {
            return Ok(0);
        }

        for result in results {
            let operations = result.operations;
            {
                let mut guard = self
                    .builder
                    .lock()
                    .map_err(|_| IngestError::Execution("graph builder poisoned".to_string()))?;
                guard
                    .apply_ops(operations.clone())
                    .map_err(naviscope_to_ingest_error)?;
            }

            let requests = self
                .resolver
                .resolve_stubs(&operations, self.routes.as_ref());
            if !requests.is_empty() {
                let mut pending = self
                    .pending_stubs
                    .lock()
                    .map_err(|_| IngestError::Execution("stub queue poisoned".to_string()))?;
                pending.extend(requests);
            }
        }

        Ok(1)
    }
}

#[derive(Default)]
struct NoopRuntimeMetrics;

impl RuntimeMetrics for NoopRuntimeMetrics {
    fn observe_queue_depth(&self, _queue: &'static str, _depth: usize) {}
    fn observe_throughput(&self, _stage: &'static str, _count: usize) {}
    fn observe_latency_ms(&self, _stage: &'static str, _p95_ms: u64, _p99_ms: u64) {}
    fn observe_replay_result(&self, _ok: bool) {}
}

pub fn run_source_ingest(
    base_graph: &CodeGraph,
    initial_ops: Vec<GraphOp>,
    source_files: Vec<ParsedFile>,
    resolver: Arc<IndexResolver>,
    project_context: Arc<ProjectContext>,
    routes: Arc<HashMap<String, Vec<PathBuf>>>,
    lang_caps: Arc<Vec<LanguageCaps>>,
    runtime_config: RuntimeConfig,
) -> Result<(CodeGraph, Vec<StubRequest>)> {
    let mut builder = base_graph.to_builder();
    for caps in lang_caps.iter() {
        if let Some(nc) = caps.presentation.naming_convention() {
            builder.naming_conventions.insert(caps.language.clone(), nc);
        }
    }
    builder.apply_ops(initial_ops)?;

    let shared_builder = Arc::new(Mutex::new(builder));
    let pending_stubs = Arc::new(Mutex::new(Vec::new()));

    let scheduler: naviscope_ingest::DynScheduler<IndexWorkItem, GraphOp> =
        Arc::new(IndexWorkScheduler);
    let executor: naviscope_ingest::DynExecutor<IndexWorkItem, GraphOp> =
        Arc::new(IndexWorkExecutor {
            resolver: Arc::clone(&resolver),
            project_context,
        });
    let deferred_store: naviscope_ingest::DynDeferredStore<IndexWorkItem> =
        Arc::new(InMemoryDeferredQueue::default());
    let commit_sink_impl = Arc::new(IndexWorkCommitter {
        builder: Arc::clone(&shared_builder),
        pending_stubs: Arc::clone(&pending_stubs),
        resolver,
        routes,
    });
    let commit_sink: naviscope_ingest::DynCommitSink<GraphOp> = commit_sink_impl;
    let metrics: naviscope_ingest::DynRuntimeMetrics = Arc::new(NoopRuntimeMetrics);

    let flow_config = FlowControlConfig::from(&runtime_config);
    let bus = naviscope_ingest::TokioPipelineBus;
    let channels =
        <naviscope_ingest::TokioPipelineBus as PipelineBus<IndexWorkItem, GraphOp>>::open_channels(
            &bus,
            flow_config.channel_capacity,
        );

    let intake_tx = channels.intake_tx.clone();
    let rt = tokio::runtime::Handle::current();
    rt.block_on(async move {
        let producer = async move {
            for (index, file) in source_files.into_iter().enumerate() {
                let msg = naviscope_ingest::Message {
                    msg_id: format!("src:{}", file.path().display()),
                    topic: "source-index".to_string(),
                    message_group: file.path().to_string_lossy().to_string(),
                    version: 1,
                    depends_on: Vec::new(),
                    epoch: index as u64,
                    payload: IndexWorkItem { file },
                    metadata: BTreeMap::new(),
                };
                intake_tx
                    .send(msg)
                    .await
                    .map_err(|_| IngestError::Execution("ingest intake closed".to_string()))?;
            }
            Ok::<(), IngestError>(())
        };

        let consumer = kernel::run_pipeline(
            channels,
            scheduler,
            executor,
            deferred_store,
            commit_sink,
            metrics,
            &flow_config,
        );

        let (_produced, _stats) = tokio::try_join!(producer, consumer)?;
        Ok::<(), IngestError>(())
    })
    .map_err(ingest_to_naviscope_error)?;

    let graph = {
        let mut guard = shared_builder
            .lock()
            .map_err(|_| NaviscopeError::Internal("graph builder poisoned".to_string()))?;
        let builder = std::mem::take(&mut *guard);
        builder.build()
    };

    let stubs = {
        let mut guard = pending_stubs
            .lock()
            .map_err(|_| NaviscopeError::Internal("stub queue poisoned".to_string()))?;
        std::mem::take(&mut *guard)
    };

    Ok((graph, stubs))
}

fn naviscope_to_ingest_error(err: NaviscopeError) -> IngestError {
    IngestError::Execution(err.to_string())
}

fn ingest_to_naviscope_error(err: IngestError) -> NaviscopeError {
    NaviscopeError::Internal(err.to_string())
}
