use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use naviscope_ingest::{
    DeferredStore, ExecutionResult, ExecutionStatus, IngestError, PipelineEvent, RuntimeMetrics,
    Scheduler,
};
use naviscope_plugin::{AssetEntry, AssetSource, BuildCaps, LanguageCaps, ProjectContext};

use crate::indexing::{compiler::BatchCompiler, stub_planner::StubPlanner};
use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp};

use super::IngestWorkItem;
use super::batch_tracker::BatchTracker;

pub struct PassThroughScheduler;

impl Scheduler<IngestWorkItem, GraphOp> for PassThroughScheduler {
    fn schedule(
        &self,
        messages: Vec<naviscope_ingest::Message<IngestWorkItem>>,
    ) -> Result<Vec<PipelineEvent<IngestWorkItem, GraphOp>>, IngestError> {
        Ok(messages.into_iter().map(PipelineEvent::Runnable).collect())
    }
}

pub struct IngestExecutor {
    pub build_caps: Arc<Vec<BuildCaps>>,
    pub lang_caps: Arc<Vec<LanguageCaps>>,
    pub project_context: Arc<RwLock<ProjectContext>>,
    pub routes: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
    pub current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    pub stub_cache: Arc<crate::cache::GlobalStubCache>,
}

impl naviscope_ingest::Executor<IngestWorkItem, GraphOp> for IngestExecutor {
    fn execute(
        &self,
        message: naviscope_ingest::Message<IngestWorkItem>,
    ) -> Result<Vec<PipelineEvent<IngestWorkItem, GraphOp>>, IngestError> {
        let operations = match message.payload {
            IngestWorkItem::SourceFile(file) => self.execute_source(file)?,
            IngestWorkItem::StubRequest(req) => self.execute_stub(req),
        };

        Ok(vec![PipelineEvent::Executed {
            epoch: message.epoch,
            result: ExecutionResult {
                msg_id: message.msg_id,
                status: ExecutionStatus::Done,
                operations,
                next_dependencies: Vec::new(),
                error: None,
            },
        }])
    }
}

impl IngestExecutor {
    fn execute_source(
        &self,
        file: naviscope_plugin::ParsedFile,
    ) -> Result<Vec<GraphOp>, IngestError> {
        let path = file.path().to_path_buf();

        let compiler = BatchCompiler::with_caps((*self.build_caps).clone(), (*self.lang_caps).clone());

        let context = self
            .project_context
            .read()
            .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?
            .clone();

        let mut ops = Vec::with_capacity(8);
        ops.push(GraphOp::RemovePath {
            path: Arc::from(path.as_path()),
        });
        ops.push(GraphOp::UpdateFile {
            metadata: file.file.clone(),
        });

        let source_ops = compiler
            .compile_source_batch(std::slice::from_ref(&file), &context)
            .map_err(naviscope_to_ingest_error)?;
        ops.extend(source_ops);

        let routes_snapshot = self
            .routes
            .read()
            .map_err(|_| IngestError::Execution("routes map poisoned".to_string()))?
            .clone();
        let stub_requests = StubPlanner::plan(&ops, &routes_snapshot);
        for req in stub_requests {
            ops.extend(self.execute_stub(req));
        }

        Ok(ops)
    }

    fn execute_stub(&self, req: StubRequest) -> Vec<GraphOp> {
        generate_stub_ops(
            &req,
            Arc::clone(&self.current),
            Arc::clone(&self.lang_caps),
            Arc::clone(&self.stub_cache),
        )
    }
}

pub fn generate_stub_ops(
    req: &StubRequest,
    current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    lang_caps: Arc<Vec<LanguageCaps>>,
    stub_cache: Arc<crate::cache::GlobalStubCache>,
) -> Vec<GraphOp> {
    let mut ops = Vec::new();

    // Skip if node already exists and resolved.
    let already_resolved = tokio::runtime::Handle::current().block_on(async {
        let lock = current.read().await;
        let graph = &**lock;
        if let Some(idx) = graph.find_node(&req.fqn)
            && let Some(node) = graph.get_node(idx)
        {
            return node.status == naviscope_api::models::graph::ResolutionStatus::Resolved;
        }
        false
    });
    if already_resolved {
        return ops;
    }

    for asset_path in &req.candidate_paths {
        let asset_key = crate::cache::AssetKey::from_path(asset_path).ok();

        if let Some(ref key) = asset_key
            && let Some(cached_stub) = stub_cache.lookup(key, &req.fqn)
        {
            ops.push(GraphOp::AddNode {
                data: Some(cached_stub),
            });
            break;
        }

        for caps in lang_caps.iter() {
            let Some(generator) = caps.asset.stub_generator() else {
                continue;
            };
            if !generator.can_generate(asset_path) {
                continue;
            }

            let entry = AssetEntry::new(asset_path.clone(), AssetSource::Unknown);
            match generator.generate(&req.fqn, &entry) {
                Ok(stub) => {
                    if let Some(ref key) = asset_key {
                        stub_cache.store(key, &stub);
                    }
                    ops.push(GraphOp::AddNode { data: Some(stub) });
                    break;
                }
                Err(err) => {
                    tracing::debug!("Failed to generate stub for {}: {}", req.fqn, err);
                }
            }
        }

        if !ops.is_empty() {
            break;
        }
    }

    ops
}

#[derive(Default)]
pub struct InMemoryDeferredQueue {
    items: Mutex<VecDeque<naviscope_ingest::Message<IngestWorkItem>>>,
}

impl DeferredStore<IngestWorkItem> for InMemoryDeferredQueue {
    fn push(&self, message: naviscope_ingest::Message<IngestWorkItem>) -> Result<(), IngestError> {
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
    ) -> Result<Vec<naviscope_ingest::Message<IngestWorkItem>>, IngestError> {
        let mut guard = self
            .items
            .lock()
            .map_err(|_| IngestError::Execution("deferred queue poisoned".to_string()))?;
        let mut out = Vec::new();
        for _ in 0..limit.max(1) {
            if let Some(item) = guard.pop_front() {
                out.push(item);
            } else {
                break;
            }
        }
        Ok(out)
    }

    fn notify_ready(
        &self,
        _event: naviscope_ingest::DependencyReadyEvent,
    ) -> Result<(), IngestError> {
        Ok(())
    }
}

pub struct CommitGraphSink {
    pub current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    pub naming_conventions: Arc<HashMap<String, Arc<dyn naviscope_plugin::NamingConvention>>>,
    pub batch_tracker: Arc<BatchTracker>,
}

impl naviscope_ingest::CommitSink<GraphOp> for CommitGraphSink {
    fn commit_epoch(
        &self,
        _epoch: u64,
        results: Vec<ExecutionResult<GraphOp>>,
    ) -> Result<usize, IngestError> {
        if results.is_empty() {
            return Ok(0);
        }

        let mut merged_ops = Vec::new();
        let mut completed_ids = Vec::with_capacity(results.len());

        for result in results {
            completed_ids.push(result.msg_id);
            merged_ops.extend(result.operations);
        }

        let naming_conventions = Arc::clone(&self.naming_conventions);
        let current = Arc::clone(&self.current);
        tokio::runtime::Handle::current().block_on(async move {
            let mut lock = current.write().await;
            let mut builder = (**lock).to_builder();
            for (lang, nc) in naming_conventions.iter() {
                builder
                    .naming_conventions
                    .insert(crate::model::Language::from(lang.clone()), Arc::clone(nc));
            }
            builder
                .apply_ops(merged_ops)
                .map_err(naviscope_to_ingest_error)?;
            *lock = Arc::new(builder.build());
            Ok::<(), IngestError>(())
        })?;

        for msg_id in completed_ids {
            self.batch_tracker.mark_done(&msg_id);
        }

        Ok(1)
    }
}

#[derive(Default)]
pub struct NoopRuntimeMetrics;

impl RuntimeMetrics for NoopRuntimeMetrics {
    fn observe_queue_depth(&self, _queue: &'static str, _depth: usize) {}
    fn observe_throughput(&self, _stage: &'static str, _count: usize) {}
    fn observe_latency_ms(&self, _stage: &'static str, _p95_ms: u64, _p99_ms: u64) {}
    fn observe_replay_result(&self, _ok: bool) {}
}

fn naviscope_to_ingest_error(err: crate::error::NaviscopeError) -> IngestError {
    IngestError::Execution(err.to_string())
}
