use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use naviscope_ingest::{
    DependencyRef, ExecutionResult, ExecutionStatus, Executor, IngestError, PipelineEvent,
};
use naviscope_plugin::{
    LanguageCaps, ParsedFile, ProjectContext, SourceAnalyzeArtifact, SourceCollectArtifact,
};

use crate::indexing::StubRequest;
use crate::ingest::IngestWorkItem;
use crate::model::{CodeGraph, GraphOp};

use super::stub_ops::{find_asset_for_fqn, generate_stub_ops, plan_stub_requests};

pub struct IngestExecutor {
    pub lang_caps: Arc<Vec<LanguageCaps>>,
    pub project_context: Arc<RwLock<ProjectContext>>,
    pub routes: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
    pub current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    pub stub_cache: Arc<crate::cache::GlobalStubCache>,
    pub collect_cache: Arc<Mutex<HashMap<PathBuf, Box<dyn SourceCollectArtifact>>>>,
    pub analyze_cache: Arc<Mutex<HashMap<PathBuf, Box<dyn SourceAnalyzeArtifact>>>>,
}

impl Executor<IngestWorkItem, GraphOp> for IngestExecutor {
    fn execute(
        &self,
        message: naviscope_ingest::Message<IngestWorkItem>,
    ) -> Result<Vec<PipelineEvent<IngestWorkItem, GraphOp>>, IngestError> {
        let parent_msg_id = message.msg_id.clone();
        let epoch = message.epoch;
        match message.payload.clone() {
            IngestWorkItem::SourceCollect(file) => {
                let dep_state = self.execute_collect(file.clone())?;
                let analyze_msg = naviscope_ingest::Message {
                    msg_id: next_stage_msg_id(&parent_msg_id, "collect", "analyze"),
                    topic: "source-analyze".to_string(),
                    message_group: file.path().to_string_lossy().to_string(),
                    version: 1,
                    depends_on: dep_state
                        .required_resources
                        .iter()
                        .cloned()
                        .map(|s| DependencyRef::resource(s, None))
                        .collect(),
                    epoch,
                    payload: IngestWorkItem::SourceAnalyze(file),
                    metadata: BTreeMap::new(),
                };
                Ok(vec![
                    PipelineEvent::Executed {
                        epoch,
                        result: ExecutionResult {
                            msg_id: parent_msg_id,
                            status: ExecutionStatus::Done,
                            operations: Vec::new(),
                            next_dependencies: dep_state.provided_resources,
                            error: None,
                        },
                    },
                    PipelineEvent::Deferred(analyze_msg),
                ])
            }
            IngestWorkItem::SourceAnalyze(file) => {
                self.execute_analyze(file.clone())?;
                let lower_msg = naviscope_ingest::Message {
                    msg_id: next_stage_msg_id(&parent_msg_id, "analyze", "lower"),
                    topic: "source-lower".to_string(),
                    message_group: message.message_group.clone(),
                    version: 1,
                    depends_on: vec![DependencyRef::message(parent_msg_id.clone())],
                    epoch,
                    payload: IngestWorkItem::SourceLower(file),
                    metadata: BTreeMap::new(),
                };
                Ok(vec![
                    PipelineEvent::Executed {
                        epoch,
                        result: ExecutionResult {
                            msg_id: parent_msg_id,
                            status: ExecutionStatus::Done,
                            operations: Vec::new(),
                            next_dependencies: Vec::new(),
                            error: None,
                        },
                    },
                    PipelineEvent::Deferred(lower_msg),
                ])
            }
            IngestWorkItem::SourceLower(file) => {
                let outcome = self.execute_lower(file)?;
                Ok(vec![PipelineEvent::Executed {
                    epoch,
                    result: ExecutionResult {
                        msg_id: parent_msg_id,
                        status: ExecutionStatus::Done,
                        operations: outcome.operations,
                        next_dependencies: Vec::new(),
                        error: None,
                    },
                }])
            }
            IngestWorkItem::StubRequest(req) => {
                let operations = self.execute_stub(req);
                Ok(vec![PipelineEvent::Executed {
                    epoch,
                    result: ExecutionResult {
                        msg_id: parent_msg_id,
                        status: ExecutionStatus::Done,
                        operations,
                        next_dependencies: Vec::new(),
                        error: None,
                    },
                }])
            }
        }
    }
}

struct SourceExecutionOutcome {
    operations: Vec<GraphOp>,
}

struct CollectDependencyState {
    provided_resources: Vec<DependencyRef>,
    required_resources: Vec<String>,
}

impl IngestExecutor {
    fn execute_collect(&self, file: ParsedFile) -> Result<CollectDependencyState, IngestError> {
        let caps = self
            .lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(file.path()));
        let Some(caps) = caps else {
            return Ok(CollectDependencyState {
                provided_resources: Vec::new(),
                required_resources: Vec::new(),
            });
        };

        let mut cache = self
            .collect_cache
            .lock()
            .map_err(|_| IngestError::Execution("collect cache poisoned".to_string()))?;
        if let Some(collected) = cache.get(file.path()) {
            let mut ctx = self
                .project_context
                .write()
                .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?;
            for sym in collected.collected_type_symbols() {
                ctx.symbol_table.type_symbols.insert(sym.clone());
            }
            for sym in collected.collected_method_symbols() {
                ctx.symbol_table.method_symbols.insert(sym.clone());
            }
            return Ok(CollectDependencyState {
                provided_resources: collected
                    .provided_dependency_symbols()
                    .iter()
                    .cloned()
                    .map(|s| DependencyRef::resource(s, None))
                    .collect(),
                required_resources: collected.required_dependency_symbols().to_vec(),
            });
        }

        let context = self
            .project_context
            .read()
            .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?
            .clone();
        let collected = caps
            .indexing
            .collect_source(&file, &context)
            .map_err(|e| IngestError::Execution(e.to_string()))?;
        {
            let mut ctx = self
                .project_context
                .write()
                .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?;
            for sym in collected.collected_type_symbols() {
                ctx.symbol_table.type_symbols.insert(sym.clone());
            }
            for sym in collected.collected_method_symbols() {
                ctx.symbol_table.method_symbols.insert(sym.clone());
            }
        }
        let provided_resources = collected
            .provided_dependency_symbols()
            .iter()
            .cloned()
            .map(|s| DependencyRef::resource(s, None))
            .collect();
        let required_resources = collected.required_dependency_symbols().to_vec();
        cache.insert(file.path().to_path_buf(), collected);
        Ok(CollectDependencyState {
            provided_resources,
            required_resources,
        })
    }

    fn execute_analyze(&self, file: ParsedFile) -> Result<(), IngestError> {
        let caps = self
            .lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(file.path()));
        let Some(caps) = caps else {
            return Ok(());
        };

        let context = self
            .project_context
            .read()
            .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?
            .clone();

        let collected = {
            let mut cache = self
                .collect_cache
                .lock()
                .map_err(|_| IngestError::Execution("collect cache poisoned".to_string()))?;
            if let Some(c) = cache.remove(file.path()) {
                c
            } else {
                caps.indexing
                    .collect_source(&file, &context)
                    .map_err(|e| IngestError::Execution(e.to_string()))?
            }
        };

        let analyzed = caps
            .indexing
            .analyze_source(collected, &context)
            .map_err(|e| IngestError::Execution(e.to_string()))?;

        let mut cache = self
            .analyze_cache
            .lock()
            .map_err(|_| IngestError::Execution("analyze cache poisoned".to_string()))?;
        cache.insert(file.path().to_path_buf(), analyzed);
        Ok(())
    }

    fn execute_lower(&self, file: ParsedFile) -> Result<SourceExecutionOutcome, IngestError> {
        let caps = self
            .lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(file.path()));
        let Some(caps) = caps else {
            return Ok(SourceExecutionOutcome {
                operations: Vec::new(),
            });
        };
        let context = self
            .project_context
            .read()
            .map_err(|_| IngestError::Execution("project context poisoned".to_string()))?
            .clone();

        let analyzed = {
            let mut cache = self
                .analyze_cache
                .lock()
                .map_err(|_| IngestError::Execution("analyze cache poisoned".to_string()))?;
            if let Some(a) = cache.remove(file.path()) {
                a
            } else {
                let collected = caps
                    .indexing
                    .collect_source(&file, &context)
                    .map_err(|e| IngestError::Execution(e.to_string()))?;
                caps.indexing
                    .analyze_source(collected, &context)
                    .map_err(|e| IngestError::Execution(e.to_string()))?
            }
        };

        let unit = caps
            .indexing
            .lower_source(analyzed, &context)
            .map_err(|e| IngestError::Execution(e.to_string()))?;

        let path = file.path().to_path_buf();

        let mut ops = Vec::with_capacity(8);
        ops.push(GraphOp::RemovePath {
            path: Arc::from(path.as_path()),
        });
        ops.push(GraphOp::UpdateFile {
            metadata: file.file.clone(),
        });

        let deferred_targets: Vec<String> =
            unit.deferred_symbols.into_iter().map(|d| d.target).collect();
        ops.extend(unit.ops);

        let routes_snapshot = self
            .routes
            .read()
            .map_err(|_| IngestError::Execution("routes map poisoned".to_string()))?
            .clone();

        let stub_requests = plan_stub_requests(&ops, &routes_snapshot);
        for req in stub_requests {
            ops.extend(self.execute_stub(req));
        }

        let deferred_stub_requests =
            deferred_targets_to_stub_requests(&deferred_targets, &routes_snapshot);
        for req in deferred_stub_requests {
            ops.extend(self.execute_stub(req));
        }

        Ok(SourceExecutionOutcome { operations: ops })
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

fn next_stage_msg_id(current_msg_id: &str, from: &str, to: &str) -> String {
    let suffix = format!(":{from}");
    if let Some(base) = current_msg_id.strip_suffix(&suffix) {
        format!("{base}:{to}")
    } else {
        format!("{current_msg_id}:{to}")
    }
}

fn deferred_targets_to_stub_requests(
    deferred_targets: &[String],
    routes: &HashMap<String, Vec<PathBuf>>,
) -> Vec<StubRequest> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for target in deferred_targets {
        if let Some(paths) = find_asset_for_fqn(target, routes) {
            if seen.insert(target.clone()) {
                out.push(StubRequest {
                    fqn: target.clone(),
                    candidate_paths: paths.clone(),
                });
            }
        }
    }

    out
}
