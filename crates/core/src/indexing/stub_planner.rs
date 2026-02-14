use crate::indexing::StubRequest;
use crate::model::GraphOp;
use naviscope_api::models::graph::NodeSource;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

pub struct StubPlanner;

impl StubPlanner {
    pub fn plan(ops: &[GraphOp], routes: &HashMap<String, Vec<PathBuf>>) -> Vec<StubRequest> {
        let mut requests = Vec::new();
        let mut seen_fqns = HashSet::new();

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
            if let Some(paths) = Self::find_asset_for_fqn(&fqn, routes) {
                requests.push(StubRequest {
                    fqn,
                    candidate_paths: paths.clone(),
                });
            }
        }
        requests
    }

    fn find_asset_for_fqn<'a>(
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
}
