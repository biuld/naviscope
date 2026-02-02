use dashmap::DashMap;
use lasso::ThreadedRodeo;
use naviscope_api::models::graph::NodeKind;
use naviscope_api::models::symbol::NodeId;
pub use naviscope_api::models::symbol::{FqnId, FqnInterner, FqnNode, FqnReader, Symbol};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Structured FQN Storage and Manager (Thread-safe Propeller Edition)
#[derive(Debug, Clone)]
pub struct FqnManager {
    pub rodeo: Arc<ThreadedRodeo>,
    pub nodes: Arc<DashMap<FqnId, FqnNode>>,
    pub lookup: Arc<DashMap<(Option<FqnId>, Symbol, NodeKind), FqnId>>,
    pub next_id: Arc<std::sync::atomic::AtomicU32>,
}

impl Default for FqnManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FqnManager {
    pub fn new() -> Self {
        Self {
            rodeo: Arc::new(ThreadedRodeo::new()),
            nodes: Arc::new(DashMap::new()),
            lookup: Arc::new(DashMap::new()),
            next_id: Arc::new(std::sync::atomic::AtomicU32::new(1)),
        }
    }

    pub fn with_rodeo(rodeo: Arc<ThreadedRodeo>) -> Self {
        Self {
            rodeo,
            nodes: Arc::new(DashMap::new()),
            lookup: Arc::new(DashMap::new()),
            next_id: Arc::new(std::sync::atomic::AtomicU32::new(1)),
        }
    }

    pub fn get_by_id(&self, id: FqnId) -> Option<FqnNode> {
        self.nodes.get(&id).map(|n| n.clone())
    }

    /// Try to find a child node with the given name under the given parent.
    /// Since we don't know the Kind, we have to try potential Kinds.
    pub fn find_child(&self, parent: Option<FqnId>, name: &str) -> Vec<FqnId> {
        let symbol = if let Some(s) = self.rodeo.get(name) {
            Symbol(s)
        } else {
            return Vec::new();
        };

        // Heuristic: Check common kinds.
        //Ideally `NodeKind` would provide an iterator, but we can hardcode the list here based on what we know.
        let kinds = [
            NodeKind::Package,
            NodeKind::Class,
            NodeKind::Interface,
            NodeKind::Method,
            NodeKind::Field,
            NodeKind::Module,
            NodeKind::Enum,
            NodeKind::Annotation,
            NodeKind::Constructor,
            NodeKind::Project,
        ];

        let mut results = Vec::new();
        for kind in &kinds {
            let key = (parent, symbol, kind.clone());
            if let Some(id) = self.lookup.get(&key) {
                results.push(*id);
            }
        }
        results
    }

    /// Resolve a dot/colon separated string to potential FqnIds.
    /// This is an expensive operation as it involves walking the path and guessing kinds.
    pub fn resolve_fqn_string(&self, fqn: &str) -> Vec<FqnId> {
        // 1. Split the string into segments
        // We handle "." and "#" and "$" as separators.
        let segments: Vec<&str> = fqn
            .split(|c| c == '.' || c == '#' || c == '$')
            .filter(|s| !s.is_empty())
            .collect();

        if segments.is_empty() {
            return Vec::new();
        }

        // 2. Simple Tree Search
        // We maintain a list of valid current_ids. Initially [None] (root).
        let mut current_ids: Vec<Option<FqnId>> = vec![None];

        for (_i, segment) in segments.iter().enumerate() {
            let mut next_ids = Vec::new();

            for parent in current_ids {
                // Try to find children matching this segment
                let children = self.find_child(parent, segment);
                for child_id in children {
                    next_ids.push(Some(child_id));
                }
            }

            if next_ids.is_empty() {
                // Heuristic: If we fail to match a segment in the middle...
                return Vec::new();
            }

            current_ids = next_ids;
        }

        current_ids.into_iter().flatten().collect()
    }
}

impl FqnReader for FqnManager {
    fn resolve_node(&self, id: FqnId) -> Option<FqnNode> {
        self.nodes.get(&id).map(|n| n.clone())
    }

    fn resolve_atom(&self, atom: Symbol) -> &str {
        self.rodeo.resolve(&atom.0)
    }
}

impl FqnInterner for FqnManager {
    fn intern_atom(&self, name: &str) -> Symbol {
        Symbol(self.rodeo.get_or_intern(name))
    }

    fn intern_node(&self, parent: Option<FqnId>, name: &str, kind: NodeKind) -> FqnId {
        let name_sym = self.intern_atom(name);
        let key = (parent, name_sym, kind.clone());

        if let Some(id) = self.lookup.get(&key) {
            return *id;
        }

        let id = FqnId(
            self.next_id
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst),
        );

        let node = FqnNode {
            parent,
            name: name_sym,
            kind,
        };

        self.nodes.insert(id, node);
        self.lookup.insert(key, id);
        id
    }

    fn intern_node_id(&self, id: &NodeId) -> FqnId {
        match id {
            NodeId::Flat(s) => self.intern_node(None, s, NodeKind::Package),
            NodeId::Structured(parts) => {
                let mut current = None;
                for (kind, name) in parts {
                    current = Some(self.intern_node(current, name, kind.clone()));
                }
                current.expect("Empty structured ID")
            }
        }
    }
}

// Custom Serialization for FqnManager
impl Serialize for FqnManager {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        use serde::ser::SerializeStruct;
        eprintln!(
            "Serializing FqnManager: rodeo_len={}, nodes={}",
            self.rodeo.len(),
            self.nodes.len()
        );
        let mut state = serializer.serialize_struct("FqnManager", 3)?;
        state.serialize_field("rodeo", &*self.rodeo)?;

        let nodes_vec: Vec<_> = self
            .nodes
            .iter()
            .map(|entry| (*entry.key(), entry.value().clone()))
            .collect();
        state.serialize_field("nodes", &nodes_vec)?;
        state.serialize_field(
            "next_id",
            &self.next_id.load(std::sync::atomic::Ordering::Relaxed),
        )?;
        state.end()
    }
}

impl<'de> Deserialize<'de> for FqnManager {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct RawData {
            rodeo: ThreadedRodeo,
            nodes: Vec<(FqnId, FqnNode)>,
            next_id: u32,
        }

        let raw = RawData::deserialize(deserializer)?;
        eprintln!(
            "Deserialized FqnManager: rodeo_len={}, nodes={}",
            raw.rodeo.len(),
            raw.nodes.len()
        );

        let nodes_map = DashMap::new();
        let lookup_map = DashMap::new();

        for (id, node) in raw.nodes {
            nodes_map.insert(id, node.clone());
            lookup_map.insert((node.parent, node.name, node.kind.clone()), id);
        }

        Ok(FqnManager {
            rodeo: Arc::new(raw.rodeo),
            nodes: Arc::new(nodes_map),
            lookup: Arc::new(lookup_map),
            next_id: Arc::new(std::sync::atomic::AtomicU32::new(raw.next_id)),
        })
    }
}

pub type FqnStorage = FqnManager;
