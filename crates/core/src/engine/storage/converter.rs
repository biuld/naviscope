use super::model::*;
use crate::engine::graph::{CodeGraphInner, FileEntry};
use crate::model::graph::GraphNode;
use petgraph::stable_graph::NodeIndex;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

pub fn to_storage(inner: &CodeGraphInner) -> StorageGraph {
    let mut string_pool = Vec::new();
    let mut string_map = HashMap::new();
    let mut path_pool = Vec::new();
    let mut path_map = HashMap::new();

    let mut intern_str = |s: &str| -> u32 {
        *string_map.entry(s.to_string()).or_insert_with(|| {
            let id = string_pool.len() as u32;
            string_pool.push(s.to_string());
            id
        })
    };

    let mut intern_path = |p: &Path| -> u32 {
        let s = p.to_string_lossy().to_string();
        *path_map.entry(s.clone()).or_insert_with(|| {
            let id = path_pool.len() as u32;
            path_pool.push(s);
            id
        })
    };

    // Map from original NodeIndex to its index in the storage nodes vector
    let mut node_id_map = HashMap::new();
    let mut nodes = Vec::new();

    for idx in inner.topology.node_indices() {
        let node = &inner.topology[idx];
        let storage_idx = nodes.len() as u32;
        node_id_map.insert(idx, storage_idx);

        nodes.push(StorageNode {
            id_sid: intern_str(&node.id),
            name_sid: intern_str(node.name.as_str()),
            kind: node.kind.clone(),
            lang_sid: intern_str(&node.lang),
            location: node.location.as_ref().map(|loc| StorageLocation {
                path_id: intern_path(&loc.path),
                range: loc.range,
                selection_range: loc.selection_range,
            }),
            metadata: node.metadata.clone(),
        });
    }

    let edges: Vec<StorageEdge> = inner
        .topology
        .edge_indices()
        .map(|idx| {
            let (from, to) = inner.topology.edge_endpoints(idx).unwrap();
            StorageEdge {
                from: *node_id_map.get(&from).unwrap(),
                to: *node_id_map.get(&to).unwrap(),
                data: inner.topology[idx].clone(),
            }
        })
        .collect();

    let mut fqn_index: Vec<(u32, u32)> = inner
        .fqn_index
        .iter()
        .map(|(fqn, idx)| (intern_str(fqn), *node_id_map.get(idx).unwrap()))
        .collect();
    fqn_index.sort_unstable_by_key(|k| k.0);

    let mut name_index: Vec<(u32, Vec<u32>)> = inner
        .name_index
        .iter()
        .map(|(name, indices)| {
            (
                intern_str(name.as_str()),
                indices
                    .iter()
                    .map(|i| *node_id_map.get(i).unwrap())
                    .collect(),
            )
        })
        .collect();
    name_index.sort_unstable_by_key(|k| k.0);

    let mut file_index: Vec<(u32, StorageFileEntry)> = inner
        .file_index
        .iter()
        .map(|(path, entry)| {
            (
                intern_path(path),
                StorageFileEntry {
                    metadata: entry.metadata.clone(),
                    nodes: entry
                        .nodes
                        .iter()
                        .map(|i| *node_id_map.get(i).unwrap())
                        .collect(),
                },
            )
        })
        .collect();
    file_index.sort_unstable_by_key(|k| k.0);

    let mut reference_index: Vec<(u32, Vec<u32>)> = inner
        .reference_index
        .iter()
        .map(|(token, paths)| {
            (
                intern_str(token.as_str()),
                paths.iter().map(|p| intern_path(p)).collect(),
            )
        })
        .collect();
    reference_index.sort_unstable_by_key(|k| k.0);

    StorageGraph {
        version: inner.version,
        string_pool,
        path_pool,
        nodes,
        edges,
        fqn_index,
        name_index,
        file_index,
        reference_index,
    }
}

pub fn from_storage(storage: StorageGraph) -> CodeGraphInner {
    let mut topology = petgraph::stable_graph::StableDiGraph::new();

    for snode in &storage.nodes {
        let node = GraphNode {
            id: Arc::from(storage.string_pool[snode.id_sid as usize].as_str()),
            name: SmolStr::from(&storage.string_pool[snode.name_sid as usize]),
            kind: snode.kind.clone(),
            lang: Arc::from(storage.string_pool[snode.lang_sid as usize].as_str()),
            location: snode
                .location
                .as_ref()
                .map(|loc| crate::model::graph::NodeLocation {
                    path: Arc::from(Path::new(&storage.path_pool[loc.path_id as usize])),
                    range: loc.range,
                    selection_range: loc.selection_range,
                }),
            metadata: snode.metadata.clone(),
        };
        topology.add_node(node);
    }

    for sedge in storage.edges {
        topology.add_edge(
            NodeIndex::new(sedge.from as usize),
            NodeIndex::new(sedge.to as usize),
            sedge.data,
        );
    }

    let fqn_index = storage
        .fqn_index
        .into_iter()
        .map(|(sid, idx)| {
            (
                Arc::from(storage.string_pool[sid as usize].as_str()),
                NodeIndex::new(idx as usize),
            )
        })
        .collect();

    let name_index = storage
        .name_index
        .into_iter()
        .map(|(sid, indices)| {
            (
                SmolStr::from(&storage.string_pool[sid as usize]),
                indices
                    .into_iter()
                    .map(|i| NodeIndex::new(i as usize))
                    .collect(),
            )
        })
        .collect();

    let file_index = storage
        .file_index
        .into_iter()
        .map(|(pid, entry)| {
            (
                Arc::from(Path::new(&storage.path_pool[pid as usize])),
                FileEntry {
                    metadata: entry.metadata,
                    nodes: entry
                        .nodes
                        .into_iter()
                        .map(|i| NodeIndex::new(i as usize))
                        .collect(),
                },
            )
        })
        .collect();

    let reference_index = storage
        .reference_index
        .into_iter()
        .map(|(sid, paths)| {
            (
                SmolStr::from(&storage.string_pool[sid as usize]),
                paths
                    .into_iter()
                    .map(|pid| Arc::from(Path::new(&storage.path_pool[pid as usize])))
                    .collect(),
            )
        })
        .collect();

    CodeGraphInner {
        version: storage.version,
        topology,
        fqn_index,
        name_index,
        file_index,
        reference_index,
    }
}
