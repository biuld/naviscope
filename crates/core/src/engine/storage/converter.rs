use super::model::*;
use super::pool::GLOBAL_POOL;
use crate::engine::graph::{CodeGraphInner, FileEntry};
use crate::model::{GraphNode, SymbolLocation};
use crate::plugin::MetadataPlugin;
use petgraph::stable_graph::NodeIndex;
use smol_str::SmolStr;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

struct GenericStorageContext<'a> {
    pools: &'a mut StoragePools,
    string_map: &'a mut HashMap<String, u32>,
    path_map: &'a mut HashMap<String, u32>,
}

impl<'a> StorageContext for GenericStorageContext<'a> {
    fn intern_str(&mut self, s: &str) -> u32 {
        *self.string_map.entry(s.to_string()).or_insert_with(|| {
            let id = self.pools.strings.len() as u32;
            self.pools.strings.push(s.to_string());
            id
        })
    }

    fn intern_path(&mut self, p: &Path) -> u32 {
        let s = p.to_string_lossy().to_string();
        *self.path_map.entry(s.clone()).or_insert_with(|| {
            let id = self.pools.paths.len() as u32;
            self.pools.paths.push(s);
            id
        })
    }

    fn resolve_str(&self, sid: u32) -> &str {
        &self.pools.strings[sid as usize]
    }

    fn resolve_path(&self, pid: u32) -> &Path {
        Path::new(&self.pools.paths[pid as usize])
    }
}

/// Fallback plugin that uses standard JSON encoding
struct DefaultMetadataPlugin;
impl MetadataPlugin for DefaultMetadataPlugin {}

/// Read-only context used during deserialization
struct ReadOnlyStorageContext<'a>(&'a StoragePools);

impl<'a> StorageContext for ReadOnlyStorageContext<'a> {
    fn intern_str(&mut self, _s: &str) -> u32 {
        unreachable!("Read-only context")
    }
    fn intern_path(&mut self, _p: &Path) -> u32 {
        unreachable!("Read-only context")
    }
    fn resolve_str(&self, sid: u32) -> &str {
        &self.0.strings[sid as usize]
    }
    fn resolve_path(&self, pid: u32) -> &Path {
        Path::new(&self.0.paths[pid as usize])
    }
}

pub fn to_storage(
    inner: &CodeGraphInner,
    get_plugin: impl Fn(&str) -> Option<Arc<dyn MetadataPlugin>>,
) -> StorageGraph {
    let mut pools = StoragePools::default();
    let mut string_map = HashMap::new();
    let mut path_map = HashMap::new();

    let mut ctx = GenericStorageContext {
        pools: &mut pools,
        string_map: &mut string_map,
        path_map: &mut path_map,
    };

    let default_plugin = Arc::new(DefaultMetadataPlugin);
    let mut node_id_map = HashMap::new();
    let mut nodes = Vec::new();

    for idx in inner.topology.node_indices() {
        let node = &inner.topology[idx];
        let storage_idx = nodes.len() as u32;
        node_id_map.insert(idx, storage_idx);

        let plugin = get_plugin(&node.lang).unwrap_or_else(|| default_plugin.clone());
        let metadata = plugin.intern(node.metadata.clone(), &mut ctx);

        nodes.push(StorageNode {
            id_sid: ctx.intern_str(&node.id),
            name_sid: ctx.intern_str(node.name.as_str()),
            kind: node.kind.clone(),
            lang_sid: ctx.intern_str(&node.lang),
            location: node.location.as_ref().map(|loc| StorageLocation {
                path_id: ctx.intern_path(&loc.path),
                range: loc.range,
                selection_range: loc.selection_range,
            }),
            metadata,
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

    // Re-use ctx for index pools
    let mut fqn_index: Vec<(u32, u32)> = inner
        .fqn_index
        .iter()
        .map(|(fqn, idx)| (ctx.intern_str(fqn), *node_id_map.get(idx).unwrap()))
        .collect();
    fqn_index.sort_unstable_by_key(|k| k.0);

    let mut name_index: Vec<(u32, Vec<u32>)> = inner
        .name_index
        .iter()
        .map(|(name, indices)| {
            (
                ctx.intern_str(name.as_str()),
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
                ctx.intern_path(path),
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
                ctx.intern_str(token.as_str()),
                paths.iter().map(|p| ctx.intern_path(p)).collect(),
            )
        })
        .collect();
    reference_index.sort_unstable_by_key(|k| k.0);

    StorageGraph {
        version: inner.version,
        pools,
        nodes,
        edges,
        fqn_index,
        name_index,
        file_index,
        reference_index,
    }
}

pub fn from_storage(
    storage: StorageGraph,
    get_plugin: impl Fn(&str) -> Option<Arc<dyn MetadataPlugin>>,
) -> CodeGraphInner {
    let mut topology = petgraph::stable_graph::StableDiGraph::new();
    let default_plugin = Arc::new(DefaultMetadataPlugin);
    
    let pools = &storage.pools;
    let ctx = ReadOnlyStorageContext(pools);

    for snode in &storage.nodes {
        let fqn_str = &pools.strings[snode.id_sid as usize];
        let fqn: Arc<str> = GLOBAL_POOL.intern_str(fqn_str);
        let lang = &pools.strings[snode.lang_sid as usize];
        
        let plugin = get_plugin(lang).unwrap_or_else(|| default_plugin.clone());
        let metadata = plugin.resolve(snode.metadata.clone(), &ctx);

        let node = GraphNode {
            id: fqn.clone(),
            name: SmolStr::from(&pools.strings[snode.name_sid as usize]),
            kind: snode.kind.clone(),
            lang: GLOBAL_POOL.intern_str(lang),
            location: snode.location.as_ref().map(|loc| SymbolLocation {
                path: GLOBAL_POOL.intern_path(Path::new(&pools.paths[loc.path_id as usize])),
                range: loc.range,
                selection_range: loc.selection_range,
            }),
            metadata,
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
                GLOBAL_POOL.intern_str(&pools.strings[sid as usize]),
                NodeIndex::new(idx as usize),
            )
        })
        .collect();

    let name_index = storage
        .name_index
        .into_iter()
        .map(|(sid, indices)| {
            (
                SmolStr::from(&pools.strings[sid as usize]),
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
                GLOBAL_POOL.intern_path(Path::new(&pools.paths[pid as usize])),
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
                SmolStr::from(&pools.strings[sid as usize]),
                paths
                    .into_iter()
                    .map(|pid| GLOBAL_POOL.intern_path(Path::new(&pools.paths[pid as usize])))
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
