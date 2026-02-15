use std::sync::Arc;
use std::{collections::HashMap, path::PathBuf};

use naviscope_api::models::EdgeType;
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
        let entry = AssetEntry::new(asset_path.clone(), AssetSource::Unknown);
        let asset_key = crate::cache::AssetKey::from_path(asset_path).ok();

        for caps in lang_caps.iter() {
            let Some(generator) = caps.asset.stub_generator() else {
                continue;
            };
            if !generator.can_generate(asset_path) {
                continue;
            }
            let cached_primary = asset_key
                .as_ref()
                .and_then(|k| stub_cache.lookup(k, &req.fqn));

            match generator.generate_stubs(&req.fqn, &entry) {
                Ok(mut nodes) => {
                    if let Some(cached) = cached_primary {
                        let cached_fqn = cached.id.to_string();
                        if !nodes.iter().any(|n| n.id.to_string() == cached_fqn) {
                            nodes.insert(0, cached);
                        }
                    }

                    if nodes.is_empty() {
                        continue;
                    }

                    let primary_fqn = nodes
                        .iter()
                        .find(|n| n.id.to_string() == req.fqn)
                        .map(|n| n.id.to_string())
                        .unwrap_or_else(|| nodes[0].id.to_string());

                    if let Some(ref key) = asset_key
                        && let Some(primary) = nodes.iter().find(|n| n.id.to_string() == req.fqn)
                    {
                        stub_cache.store(key, primary);
                    }

                    let mut seen = std::collections::HashSet::new();
                    for node in nodes {
                        let fqn = node.id.to_string();
                        if !seen.insert(fqn.clone()) {
                            continue;
                        }
                        ops.push(GraphOp::AddNode { data: Some(node) });
                        if fqn != primary_fqn {
                            ops.push(GraphOp::AddEdge {
                                from_id: naviscope_api::models::symbol::NodeId::Flat(
                                    primary_fqn.clone(),
                                ),
                                to_id: naviscope_api::models::symbol::NodeId::Flat(fqn),
                                edge: naviscope_api::models::GraphEdge::new(EdgeType::Contains),
                            });
                        }
                    }
                    break;
                }
                Err(err) => tracing::debug!("Failed to generate stub for {}: {}", req.fqn, err),
            };
        }

        if !ops.is_empty() {
            break;
        }
    }

    ops
}
