use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use naviscope_plugin::{
    LanguageCaps, ParsedFile, ProjectContext, SourceAnalyzeArtifact, SourceCollectArtifact,
};

use crate::error::{NaviscopeError, Result};
use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp};

use super::stub_ops::{find_asset_for_fqn, plan_stub_requests, resolve_stub_requests};

pub struct SourcePhaseExecutor {
    pub lang_caps: Arc<Vec<LanguageCaps>>,
    pub project_context: Arc<RwLock<ProjectContext>>,
    pub routes: Arc<RwLock<HashMap<String, Vec<PathBuf>>>>,
    pub current: Arc<tokio::sync::RwLock<Arc<CodeGraph>>>,
    pub stub_cache: Arc<crate::cache::GlobalStubCache>,
    pub collect_cache: Arc<Mutex<HashMap<PathBuf, Box<dyn SourceCollectArtifact>>>>,
    pub analyze_cache: Arc<Mutex<HashMap<PathBuf, Box<dyn SourceAnalyzeArtifact>>>>,
    pub collect_cache_limit: usize,
    pub analyze_cache_limit: usize,
}

pub struct SourceLowerOutput {
    pub ops: Vec<GraphOp>,
    pub stub_requests: Vec<StubRequest>,
}

impl SourcePhaseExecutor {
    pub fn collect_file(&self, file: &ParsedFile) -> Result<()> {
        let caps = self
            .lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(file.path()));
        let Some(caps) = caps else {
            return Ok(());
        };

        let mut cache = self
            .collect_cache
            .lock()
            .map_err(|_| NaviscopeError::Internal("collect cache poisoned".to_string()))?;

        if let Some(collected) = cache.get(file.path()) {
            self.merge_collected_symbols(collected.as_ref())?;
            return Ok(());
        }

        let context = self
            .project_context
            .read()
            .map_err(|_| NaviscopeError::Internal("project context poisoned".to_string()))?
            .clone();
        let collected = caps
            .indexing
            .collect_source(file, &context)
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;
        self.merge_collected_symbols(collected.as_ref())?;
        bounded_insert(
            &mut cache,
            file.path().to_path_buf(),
            collected,
            self.collect_cache_limit,
        );
        Ok(())
    }

    pub fn analyze_file(&self, file: &ParsedFile) -> Result<()> {
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
            .map_err(|_| NaviscopeError::Internal("project context poisoned".to_string()))?
            .clone();

        let collected = {
            let mut cache = self
                .collect_cache
                .lock()
                .map_err(|_| NaviscopeError::Internal("collect cache poisoned".to_string()))?;
            if let Some(c) = cache.remove(file.path()) {
                c
            } else {
                caps.indexing
                    .collect_source(file, &context)
                    .map_err(|e| NaviscopeError::Internal(e.to_string()))?
            }
        };

        let analyzed = caps
            .indexing
            .analyze_source(collected, &context)
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

        let mut cache = self
            .analyze_cache
            .lock()
            .map_err(|_| NaviscopeError::Internal("analyze cache poisoned".to_string()))?;
        bounded_insert(
            &mut cache,
            file.path().to_path_buf(),
            analyzed,
            self.analyze_cache_limit,
        );
        Ok(())
    }

    pub fn lower_file(&self, file: &ParsedFile) -> Result<SourceLowerOutput> {
        let caps = self
            .lang_caps
            .iter()
            .find(|c| c.matcher.supports_path(file.path()));
        let Some(caps) = caps else {
            return Ok(SourceLowerOutput {
                ops: Vec::new(),
                stub_requests: Vec::new(),
            });
        };

        let context = self
            .project_context
            .read()
            .map_err(|_| NaviscopeError::Internal("project context poisoned".to_string()))?
            .clone();

        let analyzed = {
            let mut cache = self
                .analyze_cache
                .lock()
                .map_err(|_| NaviscopeError::Internal("analyze cache poisoned".to_string()))?;
            if let Some(a) = cache.remove(file.path()) {
                a
            } else {
                let collected = caps
                    .indexing
                    .collect_source(file, &context)
                    .map_err(|e| NaviscopeError::Internal(e.to_string()))?;
                caps.indexing
                    .analyze_source(collected, &context)
                    .map_err(|e| NaviscopeError::Internal(e.to_string()))?
            }
        };

        let unit = caps
            .indexing
            .lower_source(analyzed, &context)
            .map_err(|e| NaviscopeError::Internal(e.to_string()))?;

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
            .map_err(|_| NaviscopeError::Internal("routes map poisoned".to_string()))?
            .clone();

        let mut stub_requests = plan_stub_requests(&ops, &routes_snapshot);
        let deferred_stub_requests =
            deferred_targets_to_stub_requests(&deferred_targets, &routes_snapshot);
        stub_requests.extend(deferred_stub_requests);

        Ok(SourceLowerOutput { ops, stub_requests })
    }

    pub fn stub_phase(&self, requests: Vec<StubRequest>) -> Vec<GraphOp> {
        resolve_stub_requests(
            requests,
            Arc::clone(&self.current),
            Arc::clone(&self.lang_caps),
            Arc::clone(&self.stub_cache),
        )
    }

    fn merge_collected_symbols(&self, collected: &dyn SourceCollectArtifact) -> Result<()> {
        let mut ctx = self
            .project_context
            .write()
            .map_err(|_| NaviscopeError::Internal("project context poisoned".to_string()))?;
        for sym in collected.collected_type_symbols() {
            ctx.symbol_table.type_symbols.insert(sym.clone());
        }
        for sym in collected.collected_method_symbols() {
            ctx.symbol_table.method_symbols.insert(sym.clone());
        }
        Ok(())
    }
}

fn bounded_insert<T>(cache: &mut HashMap<PathBuf, T>, key: PathBuf, value: T, limit: usize) {
    let cap = limit.max(1);
    if cache.len() >= cap {
        if let Some(evict_key) = cache.keys().next().cloned() {
            cache.remove(&evict_key);
        }
    }
    cache.insert(key, value);
}

fn deferred_targets_to_stub_requests(
    deferred_targets: &[String],
    routes: &HashMap<String, Vec<PathBuf>>,
) -> Vec<StubRequest> {
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for target in deferred_targets {
        if let Some(paths) = find_asset_for_fqn(target, routes)
            && seen.insert(target.clone())
        {
            out.push(StubRequest {
                fqn: target.clone(),
                candidate_paths: paths.clone(),
            });
        }
    }

    out
}
