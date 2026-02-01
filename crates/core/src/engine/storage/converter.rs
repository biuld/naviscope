use super::model::*;
use crate::engine::graph::{CodeGraphInner, FileEntry};
use crate::model::{GraphNode, InternedLocation};
use crate::plugin::MetadataPlugin;
use lasso::{Key, Rodeo, Spur};
use naviscope_api::models::symbol::Symbol;
use petgraph::stable_graph::NodeIndex;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

struct GenericStorageContext<'a> {
    rodeo: &'a mut Rodeo,
}

impl<'a> StorageContext for GenericStorageContext<'a> {
    fn intern_str(&mut self, s: &str) -> u32 {
        self.rodeo.get_or_intern(s).into_usize() as u32
    }

    fn intern_path(&mut self, p: &Path) -> u32 {
        let s = p.to_string_lossy();
        self.rodeo.get_or_intern(s.as_ref()).into_usize() as u32
    }

    fn resolve_str(&self, sid: u32) -> &str {
        let spur = Spur::try_from_usize(sid as usize).unwrap();
        self.rodeo.resolve(&spur)
    }

    fn resolve_path(&self, pid: u32) -> &Path {
        let spur = Spur::try_from_usize(pid as usize).unwrap();
        Path::new(self.rodeo.resolve(&spur))
    }
}

/// Fallback plugin that uses standard JSON encoding
struct DefaultMetadataPlugin;
impl MetadataPlugin for DefaultMetadataPlugin {}

/// Read-only context used during deserialization
struct ReadOnlyStorageContext<'a>(&'a Rodeo);

impl<'a> StorageContext for ReadOnlyStorageContext<'a> {
    fn intern_str(&mut self, _s: &str) -> u32 {
        unreachable!("Read-only context")
    }
    fn intern_path(&mut self, _p: &Path) -> u32 {
        unreachable!("Read-only context")
    }
    fn resolve_str(&self, sid: u32) -> &str {
        let spur = Spur::try_from_usize(sid as usize).unwrap();
        self.0.resolve(&spur)
    }
    fn resolve_path(&self, pid: u32) -> &Path {
        let spur = Spur::try_from_usize(pid as usize).unwrap();
        Path::new(self.0.resolve(&spur))
    }
}

pub fn to_storage(
    inner: &CodeGraphInner,
    get_plugin: impl Fn(&str) -> Option<Arc<dyn MetadataPlugin>>,
) -> StorageGraph {
    let mut rodeo = inner.symbols.clone();

    let mut ctx = GenericStorageContext { rodeo: &mut rodeo };

    let default_plugin = Arc::new(DefaultMetadataPlugin);
    let mut node_id_map = HashMap::new();
    let mut nodes = Vec::new();

    for idx in inner.topology.node_indices() {
        let node = &inner.topology[idx];
        let storage_idx = nodes.len() as u32;
        node_id_map.insert(idx, storage_idx);

        // Resolve language string for plugin lookup
        let lang_str = ctx.resolve_str(node.lang.0.into_usize() as u32).to_string();
        let plugin = get_plugin(&lang_str).unwrap_or_else(|| default_plugin.clone());
        let metadata = plugin.intern(node.metadata.clone(), &mut ctx);

        nodes.push(StorageNode {
            id_sid: node.id.0.into_usize() as u32,
            name_sid: node.name.0.into_usize() as u32,
            kind: node.kind.clone(),
            lang_sid: node.lang.0.into_usize() as u32,
            location: node.location.as_ref().map(|loc| StorageLocation {
                path_id: loc.path.0.into_usize() as u32,
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

    let mut fqn_index: Vec<(u32, u32)> = inner
        .fqn_index
        .iter()
        .map(|(fqn, idx)| (fqn.0.into_usize() as u32, *node_id_map.get(idx).unwrap()))
        .collect();
    fqn_index.sort_unstable_by_key(|k| k.0);

    let mut name_index: Vec<(u32, Vec<u32>)> = inner
        .name_index
        .iter()
        .map(|(name, indices)| {
            (
                name.0.into_usize() as u32,
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
                path.0.into_usize() as u32,
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
                token.0.into_usize() as u32,
                paths.iter().map(|p| p.0.into_usize() as u32).collect(),
            )
        })
        .collect();
    reference_index.sort_unstable_by_key(|k| k.0);

    StorageGraph {
        version: inner.version,
        rodeo,
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

    let rodeo = storage.rodeo;
    let ctx = ReadOnlyStorageContext(&rodeo);

    for snode in &storage.nodes {
        let lang_str = ctx.resolve_str(snode.lang_sid).to_string();
        let plugin = get_plugin(&lang_str).unwrap_or_else(|| default_plugin.clone());
        let metadata = plugin.resolve(snode.metadata.clone(), &ctx);

        let node = GraphNode {
            id: Symbol(Spur::try_from_usize(snode.id_sid as usize).unwrap()),
            name: Symbol(Spur::try_from_usize(snode.name_sid as usize).unwrap()),
            kind: snode.kind.clone(),
            lang: Symbol(Spur::try_from_usize(snode.lang_sid as usize).unwrap()),
            location: snode.location.as_ref().map(|loc| InternedLocation {
                path: Symbol(Spur::try_from_usize(loc.path_id as usize).unwrap()),
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
                Symbol(Spur::try_from_usize(sid as usize).unwrap()),
                NodeIndex::new(idx as usize),
            )
        })
        .collect();

    let name_index = storage
        .name_index
        .into_iter()
        .map(|(sid, indices)| {
            (
                Symbol(Spur::try_from_usize(sid as usize).unwrap()),
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
                Symbol(Spur::try_from_usize(pid as usize).unwrap()),
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
                Symbol(Spur::try_from_usize(sid as usize).unwrap()),
                paths
                    .into_iter()
                    .map(|pid| Symbol(Spur::try_from_usize(pid as usize).unwrap()))
                    .collect(),
            )
        })
        .collect();

    CodeGraphInner {
        instance_id: 0, // Will be updated when wrapped in CodeGraph
        version: storage.version,
        topology,
        symbols: rodeo,
        fqn_index,
        name_index,
        file_index,
        reference_index,
    }
}
