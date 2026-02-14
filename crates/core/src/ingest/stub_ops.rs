use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use naviscope_api::models::graph::NodeSource;
use naviscope_plugin::{AssetEntry, AssetSource, LanguageCaps};

use crate::indexing::StubRequest;
use crate::model::{CodeGraph, GraphOp};

pub fn plan_stub_requests(
    ops: &[GraphOp],
    routes: &HashMap<String, Vec<PathBuf>>,
) -> Vec<StubRequest> {
    let mut requests = Vec::new();
    let mut seen_fqns = std::collections::HashSet::new();

    for op in ops {
        match op {
            GraphOp::AddEdge { to_id, .. } => {
                seen_fqns.insert(to_id.to_string());
            }
            GraphOp::AddNode {
                data: Some(node_data),
            } if node_data.source == NodeSource::External => {
                seen_fqns.insert(node_data.id.to_string());
            }
            _ => {}
        }
    }

    if seen_fqns.is_empty() || routes.is_empty() {
        return requests;
    }

    for fqn in seen_fqns {
        if let Some(paths) = find_asset_for_fqn(&fqn, routes) {
            requests.push(StubRequest {
                fqn,
                candidate_paths: paths.clone(),
            });
        }
    }
    requests
}

pub fn find_asset_for_fqn<'a>(
    fqn: &str,
    routes: &'a HashMap<String, Vec<PathBuf>>,
) -> Option<&'a Vec<PathBuf>> {
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
