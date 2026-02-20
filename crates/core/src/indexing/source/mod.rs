mod executor;
mod flow_control;
mod stub_ops;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};

use naviscope_plugin::{LanguageCaps, NamingConvention, ParsedFile, ProjectContext};
use rayon::prelude::*;

use crate::error::{NaviscopeError, Result};
use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp, Language};

use executor::{SourceLowerOutput, SourcePhaseExecutor};
use flow_control::SourceFlowControl;
use stub_ops::resolve_stub_requests;
pub use stub_ops::plan_stub_requests;

pub struct SourceCompiler {
    inflight_compiles: AtomicUsize,
    completed_source_epochs: AtomicU64,
    pending_stub_requests: Arc<Mutex<Vec<StubRequest>>>,
    flow_control: SourceFlowControl,
}

impl SourceCompiler {
    pub fn new() -> Self {
        Self {
            inflight_compiles: AtomicUsize::new(0),
            completed_source_epochs: AtomicU64::new(0),
            pending_stub_requests: Arc::new(Mutex::new(Vec::new())),
            flow_control: SourceFlowControl::default(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn compile_source_files(
        &self,
        base_graph: CodeGraph,
        source_files: Vec<ParsedFile>,
        project_context: ProjectContext,
        routes: HashMap<String, Vec<PathBuf>>,
        current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
        naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> Result<CodeGraph> {
        if source_files.is_empty() {
            return Ok(base_graph);
        }

        self.inflight_compiles.fetch_add(1, Ordering::AcqRel);
        let _compile_guard = CompileGuard {
            inflight_compiles: &self.inflight_compiles,
        };
        let phase_ops = tokio::task::spawn_blocking({
            let pending_queue = Arc::clone(&self.pending_stub_requests);
            let phase_current = Arc::clone(&current);
            let phase_lang_caps = Arc::clone(&lang_caps);
            let phase_stub_cache = Arc::clone(&stub_cache);
            let flow = self.flow_control;
            move || {
                run_source_phases_blocking(
                    source_files,
                    project_context,
                    routes,
                    pending_queue,
                    phase_current,
                    phase_lang_caps,
                    phase_stub_cache,
                    flow,
                )
            }
        })
        .await
        .map_err(|e| NaviscopeError::Internal(e.to_string()))??;

        let next_graph = apply_ops_to_graph(base_graph, naming_conventions, phase_ops)?;
        self.completed_source_epochs.fetch_add(1, Ordering::AcqRel);
        Ok(next_graph)
    }

    pub fn try_submit_or_enqueue_stub_request(
        &self,
        req: StubRequest,
        current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
        naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,
        lang_caps: Arc<Vec<LanguageCaps>>,
        stub_cache: Arc<crate::cache::GlobalStubCache>,
    ) -> bool {
        if let Ok(mut pending) = self.pending_stub_requests.lock() {
            pending.push(req);
        } else {
            return false;
        }

        // No completed source phase yet: queue only (replayed in next compile).
        if self.completed_source_epochs.load(Ordering::Acquire) == 0 {
            return true;
        }
        // Source phase in progress: queue only (drained inside phase).
        if self.inflight_compiles.load(Ordering::Acquire) > 0 {
            return true;
        }

        let queued = Self::drain_pending_stub_requests(&self.pending_stub_requests);
        if queued.is_empty() {
            return true;
        }

        let ops = resolve_stub_requests(queued, current.clone(), lang_caps, stub_cache);
        if ops.is_empty() {
            return true;
        }

        apply_ops_to_current(current, naming_conventions, ops).is_ok()
    }

    fn drain_pending_stub_requests(queue: &Arc<Mutex<Vec<StubRequest>>>) -> Vec<StubRequest> {
        match queue.lock() {
            Ok(mut pending) => pending.drain(..).collect(),
            Err(_) => Vec::new(),
        }
    }
}

fn run_source_phases_blocking(
    source_files: Vec<ParsedFile>,
    project_context: ProjectContext,
    routes: HashMap<String, Vec<PathBuf>>,
    pending_stub_requests: Arc<Mutex<Vec<StubRequest>>>,
    current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    lang_caps: Arc<Vec<LanguageCaps>>,
    stub_cache: Arc<crate::cache::GlobalStubCache>,
    flow: SourceFlowControl,
) -> Result<Vec<GraphOp>> {
    let mut queued_stub_requests =
        SourceCompiler::drain_pending_stub_requests(&pending_stub_requests);

    let executor = Arc::new(SourcePhaseExecutor {
        lang_caps,
        project_context: Arc::new(RwLock::new(project_context)),
        routes: Arc::new(RwLock::new(routes)),
        current,
        stub_cache,
        collect_cache: Arc::new(Mutex::new(HashMap::new())),
        analyze_cache: Arc::new(Mutex::new(HashMap::new())),
        collect_cache_limit: flow.collect_cache_limit,
        analyze_cache_limit: flow.analyze_cache_limit,
    });

    let thread_pool = rayon::ThreadPoolBuilder::new()
        .num_threads(flow.max_parallelism.max(1))
        .build()
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

    let collect_results: Vec<Result<()>> = thread_pool.install(|| {
        source_files
            .par_iter()
            .map(|file| executor.collect_file(file))
            .collect()
    });
    for result in collect_results {
        result?;
    }

    let analyze_results: Vec<Result<()>> = thread_pool.install(|| {
        source_files
            .par_iter()
            .map(|file| executor.analyze_file(file))
            .collect()
    });
    for result in analyze_results {
        result?;
    }

    let lowered_results: Vec<Result<SourceLowerOutput>> = thread_pool.install(|| {
        source_files
            .par_iter()
            .map(|file| executor.lower_file(file))
            .collect()
    });

    let mut ops = Vec::new();
    let mut stub_requests = Vec::new();
    for result in lowered_results {
        let output = result?;
        ops.extend(output.ops);
        stub_requests.extend(output.stub_requests);
    }
    queued_stub_requests.extend(stub_requests);
    queued_stub_requests.extend(SourceCompiler::drain_pending_stub_requests(
        &pending_stub_requests,
    ));
    let stub_ops = executor.stub_phase(queued_stub_requests);
    if !stub_ops.is_empty() {
        ops.extend(stub_ops);
    }

    Ok(ops)
}

fn apply_ops_to_graph(
    base_graph: CodeGraph,
    naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,
    ops: Vec<GraphOp>,
) -> Result<CodeGraph> {
    if ops.is_empty() {
        return Ok(base_graph);
    }

    let mut builder = base_graph.to_builder();
    for (lang, naming) in naming_conventions.iter() {
        builder
            .naming_conventions
            .insert(Language::new(lang.clone()), Arc::clone(naming));
    }
    builder
        .apply_ops(ops)
        .map_err(|e| NaviscopeError::Internal(e.to_string()))?;
    Ok(builder.build())
}

fn apply_ops_to_current(
    current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    naming_conventions: Arc<HashMap<String, Arc<dyn NamingConvention>>>,
    ops: Vec<GraphOp>,
) -> Result<()> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        std::thread::spawn(move || {
            handle.block_on(async move {
                let mut lock = current.write().await;
                let next = apply_ops_to_graph(lock.as_ref().clone(), naming_conventions, ops)?;
                *lock = Arc::new(next);
                Ok(())
            })
        })
            .join()
            .map_err(|_| NaviscopeError::Internal("stub apply thread panicked".to_string()))?
    } else {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;
        runtime.handle().block_on(async move {
            let mut lock = current.write().await;
            let next = apply_ops_to_graph(lock.as_ref().clone(), naming_conventions, ops)?;
            *lock = Arc::new(next);
            Ok(())
        })
    }
}

struct CompileGuard<'a> {
    inflight_compiles: &'a AtomicUsize,
}

impl Drop for CompileGuard<'_> {
    fn drop(&mut self) {
        self.inflight_compiles.fetch_sub(1, Ordering::AcqRel);
    }
}
