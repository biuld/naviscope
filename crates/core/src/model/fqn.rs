use dashmap::DashMap;
use lasso::ThreadedRodeo;
use naviscope_api::models::graph::NodeKind;
use naviscope_api::models::symbol::NodeId;
pub use naviscope_api::models::symbol::{FqnId, FqnNode, FqnReader, Symbol};
pub use naviscope_plugin::FqnInterner;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::sync::Arc;

/// Structured FQN Storage and Manager (Thread-safe Propeller Edition)
#[derive(Debug, Clone)]
pub struct FqnManager {
    pub rodeo: Arc<ThreadedRodeo>,
    pub nodes: Arc<DashMap<FqnId, FqnNode>>,
    pub lookup: Arc<DashMap<(Option<FqnId>, Symbol, NodeKind), FqnId>>,
    pub next_id: Arc<std::sync::atomic::AtomicU32>,
    /// Registry of naming conventions for polyglot resolution
    pub registry: Arc<std::sync::RwLock<NamingRegistry>>,
}

/// Registry to hold multiple naming conventions (e.g., Standard, C++, Rust).
/// The query engine will try them in order.
#[derive(Debug)]
pub struct NamingRegistry {
    conventions: Vec<Box<dyn naviscope_plugin::NamingConvention>>,
}

impl Default for NamingRegistry {
    fn default() -> Self {
        Self {
            conventions: vec![Box::new(
                naviscope_plugin::StandardNamingConvention::default(),
            )],
        }
    }
}

impl NamingRegistry {
    pub fn register(&mut self, convention: Box<dyn naviscope_plugin::NamingConvention>) {
        self.conventions.push(convention);
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Box<dyn naviscope_plugin::NamingConvention>> {
        self.conventions.iter()
    }
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
            registry: Arc::new(std::sync::RwLock::new(NamingRegistry::default())),
        }
    }

    pub fn with_rodeo(rodeo: Arc<ThreadedRodeo>) -> Self {
        Self {
            rodeo,
            nodes: Arc::new(DashMap::new()),
            lookup: Arc::new(DashMap::new()),
            next_id: Arc::new(std::sync::atomic::AtomicU32::new(1)),
            registry: Arc::new(std::sync::RwLock::new(NamingRegistry::default())),
        }
    }

    pub fn get_by_id(&self, id: FqnId) -> Option<FqnNode> {
        self.nodes.get(&id).map(|n| n.clone())
    }

    /// Register a new naming convention logic for query resolution
    pub fn register_convention(&self, convention: Box<dyn naviscope_plugin::NamingConvention>) {
        if let Ok(mut reg) = self.registry.write() {
            reg.register(convention);
        }
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

    /// Try to resolve a structured path to a single FqnId.
    /// This follows the exact path structure without guessing kinds.
    pub fn resolve_path(&self, path: &[(NodeKind, String)]) -> Option<FqnId> {
        let mut current = None;
        for (kind, name) in path {
            let symbol = if let Some(s) = self.rodeo.get(name) {
                Symbol(s)
            } else {
                return None;
            };
            let key = (current, symbol, kind.clone());
            match self.lookup.get(&key) {
                Some(id) => current = Some(*id),
                None => return None,
            }
        }
        current
    }

    /// Resolve a dot/colon separated string to potential FqnIds.
    /// Uses StandardNamingConvention for parsing, and performs intelligent lookup
    /// based on the parsed NodeKinds.
    /// Resolve a dot/colon separated string to potential FqnIds.
    /// Uses all registered NamingConventions to parse and lookup path logic.
    pub fn resolve_fqn_string(&self, fqn: &str) -> Vec<FqnId> {
        let registry = self.registry.read().unwrap();

        let mut all_results = Vec::new();

        // 1. Iterate over ALL registered conventions.
        // Different languages might parse the same string differently (or successfully/unsuccessfully)
        for convention in registry.iter() {
            let path = convention.parse_fqn(fqn, None);
            if path.is_empty() {
                continue;
            }

            // Tree Search with Convention-guided constraints
            let mut current_ids: Vec<Option<FqnId>> = vec![None];

            for (kind, name) in path {
                let mut next_ids = Vec::new();

                // IMPROVEMENT: Use the parsed kind to optimize lookup
                // If it is strictly a Member (Method/Field/Ctor), we trust the parser's judgment (due to '#').
                let is_strict_member = matches!(
                    kind,
                    NodeKind::Method | NodeKind::Field | NodeKind::Constructor
                );

                for parent in current_ids {
                    if is_strict_member {
                        // Semantic Lookup: We know it's a member, but parsing heuristics (e.g. defaulting to Method)
                        // might mismatch the actual graph node type (e.g. Field).
                        // So we try all member-like kinds.
                        if let Some(symbol) = self.rodeo.get(&name) {
                            let sym = Symbol(symbol);

                            let member_kinds =
                                [NodeKind::Method, NodeKind::Field, NodeKind::Constructor];

                            for member_kind in member_kinds {
                                let key = (parent, sym, member_kind);
                                if let Some(id) = self.lookup.get(&key) {
                                    next_ids.push(Some(*id));
                                }
                            }
                        }
                    } else {
                        // Ambiguous/Fuzzy lookup
                        let children = self.find_child(parent, &name);
                        for child_id in children {
                            next_ids.push(Some(child_id));
                        }
                    }
                }

                if next_ids.is_empty() {
                    current_ids = Vec::new();
                    break;
                }
                current_ids = next_ids;
            }

            all_results.extend(current_ids.into_iter().flatten());
        }

        // Deduplicate results if multiple conventions yield the same ID
        all_results.sort();
        all_results.dedup();

        all_results
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
        tracing::debug!(
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
        tracing::debug!(
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
            registry: Arc::new(std::sync::RwLock::new(NamingRegistry::default())), // Re-init with defaults
        })
    }
}

pub type FqnStorage = FqnManager;
